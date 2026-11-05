//! Cliente HTTP mínimo sobre WinHTTP (RF-139). Sin dependencias extra: usa el proxy y los
//! certificados del sistema.

use std::ffi::c_void;
use std::time::Duration;

use windows::Win32::Networking::WinHttp::{
    URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_RAW_HEADERS_CRLF, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

use crate::{Context as _, PlatformError, Result, wide};

/// Respuesta HTTP.
#[derive(Debug, Clone)]
pub struct Response {
    /// Código de estado.
    pub status: u32,
    /// Texto del estado (`OK`, `Not Found`).
    pub reason: String,
    /// Cabeceras de la respuesta, en orden.
    pub headers: Vec<(String, String)>,
    /// Cuerpo (se corta en `max_bytes`).
    pub body: Vec<u8>,
    /// `true` si el cuerpo se cortó en `max_bytes`.
    pub truncated: bool,
}

/// Separa las cabeceras en bruto (`HTTP/1.1 200 OK\r\nNombre: valor\r\n…`) en el texto del
/// estado y los pares `(nombre, valor)`.
fn parse_raw_headers(raw: &str) -> (String, Vec<(String, String)>) {
    let mut lines = raw.split("\r\n");
    let reason = lines
        .next()
        .and_then(|l| l.splitn(3, ' ').nth(2))
        .unwrap_or("")
        .trim()
        .to_owned();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .filter(|(k, _)| !k.is_empty())
        .collect();
    (reason, headers)
}

/// Petición HTTP.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// Método (`GET`, `POST`...).
    pub method: &'a str,
    /// URL `http://` o `https://`.
    pub url: &'a str,
    /// Cabeceras adicionales (`Nombre: valor`).
    pub headers: &'a [(String, String)],
    /// Cuerpo.
    pub body: &'a [u8],
    /// Tiempo máximo de cada fase (resolver, conectar, enviar, recibir).
    pub timeout: Duration,
    /// Tamaño máximo del cuerpo de la respuesta.
    pub max_bytes: usize,
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle de WinHTTP propio, se cierra una sola vez.
            let _ = unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn check(h: *mut c_void, context: &'static str) -> Result<Handle> {
    if h.is_null() {
        Err(windows::core::Error::from_thread()).ctx(context)
    } else {
        Ok(Handle(h))
    }
}

/// Envía la petición y lee la respuesta (bloqueante).
pub fn send(req: &Request<'_>) -> Result<Response> {
    let url = wide(req.url);
    let mut host = vec![0u16; 256];
    let mut path = vec![0u16; 4096];
    let mut parts = URL_COMPONENTS {
        dwStructSize: size_of::<URL_COMPONENTS>() as u32,
        lpszHostName: windows::core::PWSTR(host.as_mut_ptr()),
        dwHostNameLength: host.len() as u32,
        lpszUrlPath: windows::core::PWSTR(path.as_mut_ptr()),
        dwUrlPathLength: path.len() as u32,
        dwSchemeLength: 1,
        ..Default::default()
    };
    // `lpszUrlPath` incluye la query string (`dwExtraInfoLength = 0` no la separa).
    // SAFETY: buffers locales con su tamaño en caracteres.
    unsafe { WinHttpCrackUrl(&url[..url.len() - 1], 0, &mut parts) }
        .map_err(|_| PlatformError::Invalid(format!("URL no válida: {}", req.url)))?;
    let secure = parts.nScheme.0 == 2;
    let host_len = parts.dwHostNameLength as usize;
    let path_len = parts.dwUrlPathLength as usize;
    let host = wide(&String::from_utf16_lossy(&host[..host_len.min(host.len())]));
    let mut path = String::from_utf16_lossy(&path[..path_len.min(path.len())]);
    if path.is_empty() {
        path.push('/');
    }
    let path = wide(&path);
    let method = wide(&req.method.to_ascii_uppercase());
    let ms = i32::try_from(req.timeout.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: todos los punteros apuntan a buffers locales vivos durante cada llamada; los
    // handles se cierran en orden inverso al salir (Drop).
    unsafe {
        let session = check(
            WinHttpOpen(
                w!("Rukes"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ),
            "WinHttpOpen",
        )?;
        WinHttpSetTimeouts(session.0, ms, ms, ms, ms).ctx("WinHttpSetTimeouts")?;
        let connect = check(
            WinHttpConnect(session.0, PCWSTR(host.as_ptr()), parts.nPort, 0),
            "WinHttpConnect",
        )?;
        let request = check(
            WinHttpOpenRequest(
                connect.0,
                PCWSTR(method.as_ptr()),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                if secure {
                    WINHTTP_FLAG_SECURE
                } else {
                    Default::default()
                },
            ),
            "WinHttpOpenRequest",
        )?;
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
        let _ = WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&policy.to_ne_bytes()),
        );
        let headers: String = req
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}\r\n"))
            .collect();
        let headers = wide(&headers);
        let header_slice = if req.headers.is_empty() {
            None
        } else {
            Some(&headers[..headers.len() - 1])
        };
        let body_len = u32::try_from(req.body.len())
            .map_err(|_| PlatformError::Invalid("cuerpo demasiado grande".into()))?;
        WinHttpSendRequest(
            request.0,
            header_slice,
            (!req.body.is_empty()).then(|| req.body.as_ptr().cast::<c_void>()),
            body_len,
            body_len,
            0,
        )
        .ctx("WinHttpSendRequest")?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ctx("WinHttpReceiveResponse")?;
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .ctx("WinHttpQueryHeaders")?;
        // Cabeceras en bruto: primero se pide el tamaño (falla con buffer insuficiente).
        let mut raw_len = 0u32;
        let _ = WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            PCWSTR::null(),
            None,
            &mut raw_len,
            std::ptr::null_mut(),
        );
        let (reason, headers) = if raw_len > 0 {
            let mut raw = vec![0u16; (raw_len as usize).div_ceil(2) + 1];
            let mut bytes = (raw.len() * 2) as u32;
            match WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                PCWSTR::null(),
                Some(raw.as_mut_ptr().cast()),
                &mut bytes,
                std::ptr::null_mut(),
            ) {
                Ok(()) => {
                    let n = (bytes as usize / 2).min(raw.len());
                    parse_raw_headers(&String::from_utf16_lossy(&raw[..n]))
                }
                Err(_) => (String::new(), Vec::new()),
            }
        } else {
            (String::new(), Vec::new())
        };
        let mut body = Vec::new();
        let mut truncated = false;
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut avail).ctx("WinHttpQueryDataAvailable")?;
            if avail == 0 {
                break;
            }
            let start = body.len();
            let want = (avail as usize).min(req.max_bytes.saturating_sub(start));
            if want == 0 {
                truncated = true;
                break;
            }
            body.resize(start + want, 0);
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                body[start..].as_mut_ptr().cast(),
                want as u32,
                &mut read,
            )
            .ctx("WinHttpReadData")?;
            body.truncate(start + read as usize);
            if read == 0 {
                break;
            }
        }
        Ok(Response {
            status,
            reason,
            headers,
            body,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_headers_are_split() {
        let (reason, h) = parse_raw_headers(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nX-A:  b \r\n\r\n",
        );
        assert_eq!(reason, "Not Found");
        assert_eq!(
            h,
            vec![
                ("Content-Type".into(), "text/plain".into()),
                ("X-A".into(), "b".into())
            ]
        );
    }

    #[test]
    fn invalid_url_is_an_error() {
        let r = send(&Request {
            method: "GET",
            url: "no es una url",
            headers: &[],
            body: &[],
            timeout: Duration::from_secs(1),
            max_bytes: 1024,
        });
        assert!(r.is_err());
    }

    #[test]
    fn get_and_post_against_a_local_server() {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).unwrap();
                seen.push(String::from_utf8_lossy(&buf[..n]).into_owned());
                s.write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhola!",
                )
                .unwrap();
            }
            seen
        });
        let url = format!("http://127.0.0.1:{port}/a?b=1");
        let get = send(&Request {
            method: "GET",
            url: &url,
            headers: &[("X-Test".into(), "1".into())],
            body: &[],
            timeout: Duration::from_secs(5),
            max_bytes: 1024,
        })
        .unwrap();
        assert_eq!(get.status, 201);
        assert_eq!(get.reason, "Created");
        assert!(
            get.headers
                .iter()
                .any(|(k, v)| k == "Content-Length" && v == "5")
        );
        assert_eq!(get.body, b"hola!");
        assert!(!get.truncated);
        let post = send(&Request {
            method: "post",
            url: &url,
            headers: &[],
            body: b"{\"x\":1}",
            timeout: Duration::from_secs(5),
            max_bytes: 3,
        })
        .unwrap();
        assert_eq!(post.body, b"hol");
        assert!(post.truncated);
        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("GET /a?b=1 HTTP/1.1"));
        assert!(seen[0].contains("X-Test: 1"));
        assert!(seen[1].starts_with("POST /a?b=1"));
    }

    #[test]
    fn connection_refused_is_an_error_not_a_hang() {
        // Puerto local cerrado: falla enseguida sin salir a la red.
        let t = std::time::Instant::now();
        let r = send(&Request {
            method: "GET",
            url: "http://127.0.0.1:9/",
            headers: &[],
            body: &[],
            timeout: Duration::from_secs(2),
            max_bytes: 1024,
        });
        assert!(r.is_err());
        assert!(t.elapsed() < Duration::from_secs(5));
    }
}
