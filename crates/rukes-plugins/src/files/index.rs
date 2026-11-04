//! Índice compacto de archivos (RF-040, RNF-007).
//!
//! Cada entrada ocupa 12 bytes más su nombre en un único `String` compartido. Las rutas no se
//! guardan: se reconstruyen subiendo por los padres. Un padre siempre tiene un índice menor que
//! sus hijos, lo que permite borrar subárboles en una sola pasada.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::os::windows::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rukes_core::{CancelToken, Fuzzy};

const DIR: u8 = 1;
const HIDDEN: u8 = 2;
const DELETED: u8 = 4;
/// Padre de las raíces.
pub const NO_PARENT: u32 = u32::MAX;

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

const MAGIC: &[u8; 4] = b"RKFI";
const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
struct Entry {
    parent: u32,
    name_off: u32,
    name_len: u16,
    flags: u8,
}

/// Índice de archivos.
#[derive(Debug, Default, Clone)]
pub struct Index {
    entries: Vec<Entry>,
    names: String,
    deleted: usize,
}

/// Qué tipo de entradas buscar (RF-044).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Archivos y carpetas.
    Any,
    /// Solo archivos (`f:`).
    Files,
    /// Solo carpetas (`d:`).
    Dirs,
}

/// Reglas de indexado.
#[derive(Debug, Clone)]
pub struct Rules {
    exclude: GlobSet,
    include_hidden: bool,
}

impl Rules {
    /// Compila los patrones de exclusión (sin distinguir mayúsculas). Los inválidos se ignoran.
    pub fn new(exclude: &[String], include_hidden: bool) -> Self {
        let mut b = GlobSetBuilder::new();
        for pat in exclude {
            match GlobBuilder::new(pat)
                .case_insensitive(true)
                .literal_separator(true)
                .build()
            {
                Ok(g) => {
                    b.add(g);
                }
                Err(e) => {
                    tracing::warn!(pattern = %pat, error = %e, "patrón de exclusión no válido")
                }
            }
        }
        Self {
            exclude: b.build().unwrap_or_else(|_| GlobSet::empty()),
            include_hidden,
        }
    }

    /// `true` si la ruta está excluida.
    pub fn excluded(&self, path: &Path) -> bool {
        self.exclude.is_match(path)
    }
}

/// Nombre de un elemento del directorio con sus datos básicos.
struct Listed {
    name: String,
    dir: bool,
    hidden: bool,
    /// Directorio en el que no se entra (enlace o unión).
    reparse: bool,
}

fn list_dir(dir: &Path, rules: &Rules) -> Vec<Listed> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        let attrs = md.file_attributes();
        let hidden = attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
            || e.file_name().to_string_lossy().starts_with('.');
        if hidden && !rules.include_hidden {
            continue;
        }
        let path = e.path();
        if rules.excluded(&path) {
            continue;
        }
        out.push(Listed {
            name: e.file_name().to_string_lossy().into_owned(),
            dir: md.is_dir(),
            hidden,
            reparse: attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0,
        });
    }
    out
}

impl Index {
    /// Número de entradas vivas.
    pub fn len(&self) -> usize {
        self.entries.len() - self.deleted
    }

    /// `true` si no hay entradas vivas.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes reservados por el índice (aprox.).
    pub fn memory_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<Entry>() + self.names.capacity()
    }

    /// Fracción de entradas borradas (para decidir si compactar).
    pub fn garbage_ratio(&self) -> f32 {
        if self.entries.is_empty() {
            0.0
        } else {
            self.deleted as f32 / self.entries.len() as f32
        }
    }

    /// Añade una entrada y devuelve su índice.
    pub fn push(&mut self, parent: u32, name: &str, dir: bool, hidden: bool) -> u32 {
        let idx = self.entries.len() as u32;
        let name = &name[..name.len().min(u16::MAX as usize)];
        self.entries.push(Entry {
            parent,
            name_off: self.names.len() as u32,
            name_len: name.len() as u16,
            flags: (u8::from(dir) * DIR) | (u8::from(hidden) * HIDDEN),
        });
        self.names.push_str(name);
        idx
    }

    /// Nombre de la entrada.
    pub fn name(&self, idx: u32) -> &str {
        self.entries.get(idx as usize).map_or("", |e| {
            let start = e.name_off as usize;
            self.names
                .get(start..start + e.name_len as usize)
                .unwrap_or("")
        })
    }

    /// `true` si es una carpeta.
    pub fn is_dir(&self, idx: u32) -> bool {
        self.entries
            .get(idx as usize)
            .is_some_and(|e| e.flags & DIR != 0)
    }

    /// Ruta completa.
    pub fn path(&self, idx: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = idx;
        while let Some(e) = self.entries.get(cur as usize) {
            parts.push(self.name(cur));
            if e.parent == NO_PARENT || parts.len() > 512 {
                break;
            }
            cur = e.parent;
        }
        let mut p = PathBuf::new();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }

    /// Índices de las raíces vivas.
    pub fn roots(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.parent == NO_PARENT && e.flags & DELETED == 0)
            .map(|(i, _)| i as u32)
    }

    /// Hijos vivos de una entrada.
    pub fn children(&self, idx: u32) -> Vec<u32> {
        let start = idx as usize + 1;
        self.entries
            .get(start..)
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .filter(|(_, e)| e.parent == idx && e.flags & DELETED == 0)
            .map(|(i, _)| (start + i) as u32)
            .collect()
    }

    /// Busca la entrada de una ruta (sin distinguir mayúsculas).
    pub fn find(&self, path: &Path) -> Option<u32> {
        let path_s = path.to_string_lossy().to_lowercase();
        for root in self.roots() {
            let root_s = self.name(root).to_lowercase();
            let Some(rest) = path_s.strip_prefix(root_s.trim_end_matches('\\')) else {
                continue;
            };
            if !(rest.is_empty() || rest.starts_with('\\')) {
                continue;
            }
            let mut cur = root;
            for comp in rest.split('\\').filter(|c| !c.is_empty()) {
                cur = self
                    .children(cur)
                    .into_iter()
                    .find(|&c| self.name(c).to_lowercase() == comp)?;
            }
            return Some(cur);
        }
        None
    }

    /// Marca como borradas las entradas y todos sus descendientes.
    pub fn remove_subtrees(&mut self, roots: &[u32]) {
        let Some(&min) = roots.iter().min() else {
            return;
        };
        let mut gone = vec![false; self.entries.len()];
        for &r in roots {
            if let Some(g) = gone.get_mut(r as usize) {
                *g = true;
            }
        }
        for i in min as usize..self.entries.len() {
            let parent = self.entries[i].parent;
            let dead = gone[i]
                || (parent != NO_PARENT && gone.get(parent as usize).copied().unwrap_or(false));
            if dead {
                gone[i] = true;
                if self.entries[i].flags & DELETED == 0 {
                    self.entries[i].flags |= DELETED;
                    self.deleted += 1;
                }
            }
        }
    }

    /// Copia un subárbol de `sub` (cuya entrada 0 es la raíz) colgándolo de `parent`.
    pub fn graft(&mut self, parent: u32, sub: &Index) {
        let base = self.entries.len() as u32;
        for (i, e) in sub.entries.iter().enumerate() {
            if e.flags & DELETED != 0 {
                continue;
            }
            let p = if i == 0 { parent } else { base + e.parent };
            let name = sub.name(i as u32).to_owned();
            let idx = self.push(p, &name, e.flags & DIR != 0, e.flags & HIDDEN != 0);
            debug_assert_eq!(idx, base + i as u32);
        }
    }

    /// Reconstruye el índice sin entradas borradas.
    pub fn compact(&self) -> Index {
        let mut out = Index {
            entries: Vec::with_capacity(self.len()),
            names: String::with_capacity(self.names.len()),
            deleted: 0,
        };
        let mut remap = vec![NO_PARENT; self.entries.len()];
        for (i, e) in self.entries.iter().enumerate() {
            if e.flags & DELETED != 0 {
                continue;
            }
            let parent = if e.parent == NO_PARENT {
                NO_PARENT
            } else {
                match remap.get(e.parent as usize) {
                    Some(&p) if p != NO_PARENT => p,
                    _ => continue,
                }
            };
            let idx = out.push(
                parent,
                self.name(i as u32),
                e.flags & DIR != 0,
                e.flags & HIDDEN != 0,
            );
            remap[i] = idx;
        }
        out.entries.shrink_to_fit();
        out.names.shrink_to_fit();
        out
    }

    // -----------------------------------------------------------------------------------------
    // Recorrido del disco.

    /// Recorre `dir` y añade su contenido colgando de `parent`.
    pub fn scan_into(&mut self, parent: u32, dir: &Path, rules: &Rules, cancel: &CancelToken) {
        let mut stack = vec![(parent, dir.to_path_buf())];
        while let Some((idx, path)) = stack.pop() {
            if cancel.is_cancelled() {
                return;
            }
            for item in list_dir(&path, rules) {
                let child = self.push(idx, &item.name, item.dir, item.hidden);
                if item.dir && !item.reparse {
                    stack.push((child, path.join(&item.name)));
                }
            }
        }
    }

    /// Índice completo de las raíces.
    pub fn scan(roots: &[PathBuf], rules: &Rules, cancel: &CancelToken) -> Index {
        let mut idx = Index::default();
        for root in roots {
            if !root.is_dir() {
                continue;
            }
            let r = idx.push(NO_PARENT, &root.to_string_lossy(), true, false);
            idx.scan_into(r, root, rules, cancel);
        }
        idx.entries.shrink_to_fit();
        idx.names.shrink_to_fit();
        idx
    }

    /// Índice de una sola carpeta (entrada 0) con todo su contenido, para [`Index::graft`].
    pub fn scan_subtree(dir: &Path, rules: &Rules, cancel: &CancelToken) -> Index {
        let mut idx = Index::default();
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let r = idx.push(NO_PARENT, &name, true, false);
        idx.scan_into(r, dir, rules, cancel);
        idx
    }

    /// Sincroniza los hijos directos de `idx` con el disco. Las carpetas nuevas se recorren
    /// enteras; las que siguen igual no se tocan. Devuelve `true` si cambió algo.
    pub fn refresh_dir(&mut self, idx: u32, rules: &Rules, cancel: &CancelToken) -> bool {
        let dir = self.path(idx);
        let on_disk = if dir.is_dir() {
            list_dir(&dir, rules)
        } else {
            Vec::new()
        };
        let current = self.children(idx);
        let mut removed = Vec::new();
        for &c in &current {
            let name = self.name(c);
            let keep = on_disk
                .iter()
                .any(|l| l.name.eq_ignore_ascii_case(name) && l.dir == self.is_dir(c));
            if !keep {
                removed.push(c);
            }
        }
        let mut added = 0;
        let existing: Vec<String> = current
            .iter()
            .filter(|c| !removed.contains(c))
            .map(|&c| self.name(c).to_lowercase())
            .collect();
        for l in on_disk {
            if existing.contains(&l.name.to_lowercase()) {
                continue;
            }
            added += 1;
            if l.dir && !l.reparse {
                let sub = Index::scan_subtree(&dir.join(&l.name), rules, cancel);
                self.graft(idx, &sub);
            } else {
                self.push(idx, &l.name, l.dir, l.hidden);
            }
        }
        self.remove_subtrees(&removed);
        added > 0 || !removed.is_empty()
    }

    // -----------------------------------------------------------------------------------------
    // Búsqueda.

    /// Las `limit` mejores entradas para `text` (puntuación normalizada 0..1). Busca en
    /// paralelo por bloques.
    pub fn search(
        &self,
        text: &str,
        kind: Kind,
        ext: Option<&str>,
        limit: usize,
        cancel: &CancelToken,
    ) -> Vec<(u32, f32)> {
        let n = self.entries.len();
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(8);
        let threads = if n < 20_000 { 1 } else { threads };
        let chunk = n.div_ceil(threads.max(1)).max(1);
        let atoms = prefilter_atoms(text);
        let ext = ext.map(|e| format!(".{}", e.to_ascii_lowercase()));
        let run = |range: std::ops::Range<usize>| -> Vec<(u32, u32)> {
            let mut f = Fuzzy::new(text);
            let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::with_capacity(limit + 1);
            for i in range {
                if i % 4096 == 0 && cancel.is_cancelled() {
                    break;
                }
                let e = self.entries[i];
                if e.flags & DELETED != 0 || e.parent == NO_PARENT {
                    continue;
                }
                let is_dir = e.flags & DIR != 0;
                if (kind == Kind::Files && is_dir) || (kind == Kind::Dirs && !is_dir) {
                    continue;
                }
                let name = self.name(i as u32);
                if let Some(ext) = &ext
                    && (is_dir || !name.to_ascii_lowercase().ends_with(ext.as_str()))
                {
                    continue;
                }
                if !passes_prefilter(name, &atoms) {
                    continue;
                }
                let Some(s) = f.score(name) else { continue };
                // Desempate estable: a igual puntuación, la ruta más corta (menor profundidad
                // aproximada por índice) gana.
                let key = ((s * 1_000_000.0) as u32, u32::MAX - i as u32);
                if heap.len() < limit {
                    heap.push(Reverse(key));
                } else if heap.peek().is_some_and(|Reverse(min)| key > *min) {
                    heap.pop();
                    heap.push(Reverse(key));
                }
            }
            heap.into_iter().map(|Reverse(k)| k).collect()
        };
        let mut all: Vec<(u32, u32)> = if threads == 1 {
            run(0..n)
        } else {
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let range = (t * chunk).min(n)..((t + 1) * chunk).min(n);
                        s.spawn(move || run(range))
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|h| h.join().unwrap_or_default())
                    .collect()
            })
        };
        all.sort_unstable_by(|a, b| b.cmp(a));
        all.truncate(limit);
        all.into_iter()
            .map(|(s, inv)| (u32::MAX - inv, s as f32 / 1_000_000.0))
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // Caché en disco.

    /// Guarda el índice (compactado) de forma atómica.
    pub fn save(&self, path: &Path, config_hash: u64) -> std::io::Result<()> {
        let compact;
        let idx = if self.deleted > 0 {
            compact = self.compact();
            &compact
        } else {
            self
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut w = BufWriter::new(File::create(&tmp)?);
            w.write_all(MAGIC)?;
            w.write_all(&VERSION.to_le_bytes())?;
            w.write_all(&config_hash.to_le_bytes())?;
            w.write_all(&(idx.entries.len() as u32).to_le_bytes())?;
            w.write_all(&(idx.names.len() as u32).to_le_bytes())?;
            for e in &idx.entries {
                w.write_all(&e.parent.to_le_bytes())?;
                w.write_all(&[e.flags])?;
                w.write_all(&e.name_len.to_le_bytes())?;
            }
            w.write_all(idx.names.as_bytes())?;
            w.flush()?;
        }
        fs::rename(&tmp, path)
    }

    /// Carga la caché. `None` si no existe, está dañada o es de otra configuración.
    pub fn load(path: &Path, config_hash: u64) -> Option<Index> {
        let mut r = BufReader::new(File::open(path).ok()?);
        let mut head = [0u8; 4 + 4 + 8 + 4 + 4];
        r.read_exact(&mut head).ok()?;
        let u32_at = |o: usize| {
            head.get(o..o + 4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_le_bytes)
        };
        if &head[..4] != MAGIC || u32_at(4)? != VERSION {
            return None;
        }
        let hash = u64::from_le_bytes(head.get(8..16)?.try_into().ok()?);
        if hash != config_hash {
            return None;
        }
        let count = u32_at(16)? as usize;
        let names_len = u32_at(20)? as usize;
        let mut raw = vec![0u8; count.checked_mul(7)?];
        r.read_exact(&mut raw).ok()?;
        let mut names = vec![0u8; names_len];
        r.read_exact(&mut names).ok()?;
        let names = String::from_utf8(names).ok()?;
        let mut entries = Vec::with_capacity(count);
        let mut off = 0usize;
        for (i, c) in raw.chunks_exact(7).enumerate() {
            let parent = u32::from_le_bytes(c.get(0..4)?.try_into().ok()?);
            let flags = c[4] & (DIR | HIDDEN);
            let name_len = u16::from_le_bytes(c.get(5..7)?.try_into().ok()?);
            let end = off + name_len as usize;
            if (parent != NO_PARENT && parent as usize >= i)
                || end > names.len()
                || !names.is_char_boundary(off)
                || !names.is_char_boundary(end)
            {
                return None;
            }
            entries.push(Entry {
                parent,
                name_off: off as u32,
                name_len,
                flags,
            });
            off = end;
        }
        Some(Index {
            entries,
            names,
            deleted: 0,
        })
    }
}

/// Átomos de la consulta en minúsculas ASCII para el prefiltro. Los átomos con sintaxis de
/// nucleo (`^`, `$`, `!`, `'`) o caracteres no ASCII no se prefiltran.
fn prefilter_atoms(text: &str) -> Vec<Vec<u8>> {
    text.split_whitespace()
        .filter(|a| a.is_ascii() && !a.contains(['^', '$', '!', '\'', '\\']))
        .map(|a| a.bytes().map(|b| b.to_ascii_lowercase()).collect())
        .collect()
}

/// Descarta rápido los nombres que no contienen cada átomo como subsecuencia. Es conservador:
/// los nombres con caracteres no ASCII pasan siempre (nucleo normaliza acentos).
fn passes_prefilter(name: &str, atoms: &[Vec<u8>]) -> bool {
    if atoms.is_empty() || !name.is_ascii() {
        return true;
    }
    let hay = name.as_bytes();
    atoms.iter().all(|atom| {
        let mut it = hay.iter().map(u8::to_ascii_lowercase);
        atom.iter().all(|&c| it.any(|h| h == c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rukes-idx-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        for d in [
            "docs/informes",
            "proyecto/node_modules/x",
            "proyecto/src",
            ".oculta",
        ] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [
            "docs/informes/Informe anual 2026.pdf",
            "docs/notas.txt",
            "proyecto/src/main.rs",
            "proyecto/node_modules/x/index.js",
            ".oculta/secreto.txt",
            "Configuración.md",
        ] {
            fs::write(root.join(f), "x").unwrap();
        }
        root
    }

    fn rand_suffix() -> u32 {
        getrandom::u32().unwrap()
    }

    fn rules() -> Rules {
        Rules::new(&["**/node_modules".to_owned()], false)
    }

    fn names(idx: &Index, hits: &[(u32, f32)]) -> Vec<String> {
        hits.iter().map(|(i, _)| idx.name(*i).to_owned()).collect()
    }

    #[test]
    fn scans_with_exclusions_and_searches() {
        let root = temp_tree();
        let c = CancelToken::new();
        let idx = Index::scan(std::slice::from_ref(&root), &rules(), &c);
        let all: Vec<&str> = (0..idx.entries.len() as u32).map(|i| idx.name(i)).collect();
        assert!(all.contains(&"main.rs"));
        assert!(!all.contains(&"index.js"), "node_modules excluido");
        assert!(!all.contains(&"secreto.txt"), "ocultas excluidas");
        let hits = idx.search("informe", Kind::Any, None, 10, &c);
        assert_eq!(names(&idx, &hits)[0], "informes");
        let hits = idx.search("informe", Kind::Files, None, 10, &c);
        assert_eq!(names(&idx, &hits), ["Informe anual 2026.pdf"]);
        let hits = idx.search("", Kind::Any, Some("pdf"), 10, &c);
        assert_eq!(names(&idx, &hits), ["Informe anual 2026.pdf"]);
        let hits = idx.search("configuracion", Kind::Any, None, 10, &c);
        assert_eq!(names(&idx, &hits), ["Configuración.md"]);
        let main = idx.search("main.rs", Kind::Any, None, 1, &c)[0].0;
        assert_eq!(
            idx.path(main),
            root.join("proyecto").join("src").join("main.rs")
        );
        assert_eq!(
            idx.find(&root.join("PROYECTO").join("src")),
            Some(idx.entries[main as usize].parent)
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn refresh_dir_applies_changes_and_cache_roundtrips() {
        let root = temp_tree();
        let c = CancelToken::new();
        let r = rules();
        let mut idx = Index::scan(std::slice::from_ref(&root), &r, &c);
        let before = idx.len();
        // Renombrar una carpeta con contenido y crear un archivo.
        fs::rename(root.join("docs"), root.join("documentos")).unwrap();
        fs::write(root.join("nuevo.txt"), "x").unwrap();
        let root_idx = idx.find(&root).unwrap();
        assert!(idx.refresh_dir(root_idx, &r, &c));
        assert_eq!(idx.len(), before + 1);
        assert!(
            idx.find(&root.join("documentos").join("informes"))
                .is_some()
        );
        assert!(idx.find(&root.join("docs")).is_none());
        let hits = idx.search("informe anual", Kind::Files, None, 5, &c);
        assert_eq!(
            idx.path(hits[0].0),
            root.join("documentos")
                .join("informes")
                .join("Informe anual 2026.pdf")
        );
        // Sin cambios: nada que hacer.
        assert!(!idx.refresh_dir(root_idx, &r, &c));

        let cache = root.join("files.bin");
        idx.save(&cache, 42).unwrap();
        assert!(Index::load(&cache, 7).is_none(), "otra configuración");
        let loaded = Index::load(&cache, 42).unwrap();
        assert_eq!(loaded.len(), idx.len());
        let hits = loaded.search("informe anual", Kind::Files, None, 5, &c);
        assert_eq!(
            loaded.path(hits[0].0),
            root.join("documentos")
                .join("informes")
                .join("Informe anual 2026.pdf")
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn corrupt_cache_is_rejected() {
        let p = std::env::temp_dir().join(format!("rukes-bad-{}.bin", rand_suffix()));
        fs::write(&p, b"RKFI\x01\0\0\0garbage").unwrap();
        assert!(Index::load(&p, 0).is_none());
        fs::remove_file(&p).unwrap();
    }

    #[test]
    fn prefilter_is_conservative() {
        let atoms = prefilter_atoms("inf pdf");
        assert!(passes_prefilter("Informe.pdf", &atoms));
        assert!(!passes_prefilter("notas.txt", &atoms));
        assert!(passes_prefilter(
            "Información.pdf",
            &prefilter_atoms("informacion")
        ));
    }

    #[test]
    fn parallel_search_matches_serial_top() {
        let mut idx = Index::default();
        let r = idx.push(NO_PARENT, "C:\\raiz", true, false);
        for i in 0..50_000 {
            idx.push(r, &format!("archivo_{i:05}.txt"), false, false);
        }
        idx.push(r, "presupuesto final.xlsx", false, false);
        let hits = idx.search("presupuesto", Kind::Any, None, 5, &CancelToken::new());
        assert_eq!(idx.name(hits[0].0), "presupuesto final.xlsx");
        let hits = idx.search("archivo_04999", Kind::Any, None, 3, &CancelToken::new());
        assert_eq!(idx.name(hits[0].0), "archivo_04999.txt");
    }
}
