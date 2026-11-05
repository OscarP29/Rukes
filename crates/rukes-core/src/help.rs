//! Ayuda de uso de cada plugin: la muestra el comando `help <comando>` / `info <comando>`.

use crate::i18n::{lookup, tr};

/// Un ejemplo de uso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpUsage {
    /// Lo que se escribe (`vol 50`). Enter en la ayuda lo escribe en el launcher.
    pub example: String,
    /// Qué hace.
    pub description: String,
}

/// Ayuda de un plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginHelp {
    /// Resumen de una línea.
    pub summary: String,
    /// Ejemplos.
    pub usage: Vec<HelpUsage>,
    /// Notas: teclas, opciones de `config.toml`, trucos.
    pub tips: Vec<String>,
}

/// Ejemplos y notas como mucho por plugin al leerlos de los textos.
const MAX: usize = 12;

impl PluginHelp {
    /// Lee la ayuda de los textos de la interfaz, sección `[usage.<id>]`:
    ///
    /// ```toml
    /// [usage.controls]
    /// summary = "Volumen, brillo y radios"
    /// ex1 = "vol 50 | Pone el volumen al 50 %"
    /// tip1 = "Las escrituras piden Enter"
    /// ```
    ///
    /// Los ejemplos van como `ejemplo | descripción`. `None` si no hay `summary`.
    pub fn from_i18n(id: &str) -> Option<Self> {
        let summary = lookup(&format!("usage.{id}.summary"))?;
        let mut help = Self {
            summary: summary.to_owned(),
            ..Self::default()
        };
        for i in 1..=MAX {
            let Some(ex) = lookup(&format!("usage.{id}.ex{i}")) else {
                break;
            };
            let (example, description) = ex.split_once(" | ").unwrap_or((ex, ""));
            help.usage.push(HelpUsage {
                example: example.trim().to_owned(),
                description: description.trim().to_owned(),
            });
        }
        for i in 1..=MAX {
            let Some(tip) = lookup(&format!("usage.{id}.tip{i}")) else {
                break;
            };
            help.tips.push(tip.to_owned());
        }
        Some(help)
    }

    /// Texto completo (Markdown sencillo) para el panel de detalles.
    pub fn to_markdown(&self, name: &str) -> String {
        let mut s = format!("# {name}\n\n{}\n", self.summary);
        if !self.usage.is_empty() {
            s.push_str(&format!("\n## {}\n", tr("help.examples")));
            for u in &self.usage {
                s.push_str(&format!("- `{}`  {}\n", u.example, u.description));
            }
        }
        if !self.tips.is_empty() {
            s.push_str(&format!("\n## {}\n", tr("help.tips")));
            for t in &self.tips {
                s.push_str(&format!("- {t}\n"));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_examples_and_tips_from_the_texts() {
        let h = PluginHelp::from_i18n("help").expect("ayuda de help");
        assert!(!h.summary.is_empty());
        assert!(h.usage.iter().any(|u| u.example.starts_with("help")));
        assert!(h.usage.iter().all(|u| !u.description.is_empty()));
        assert!(PluginHelp::from_i18n("no-existe").is_none());
        let md = h.to_markdown("Ayuda");
        assert!(md.starts_with("# Ayuda\n"));
        assert!(md.contains("- `help"));
    }
}
