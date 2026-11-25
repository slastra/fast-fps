//! The few things that work differently in the browser version (docs/web.md).
//! Each has a native and a web side, chosen when the game is compiled:
//!
//! | | Native | Browser |
//! |---|---|---|
//! | [`load_settings`], [`save_settings`] | `config.cfg` in the config folder | `localStorage` |
//! | [`launch_lines`] | `+command` arguments | `?c=` in the page's URL |
//! | [`read_script`] | a file, read at once | fetched from the server, later |
//! | [`pointer_locked`] | what Bevy says | what the page says |
//!
//! Anything else browser-specific belongs here too, so the rest of the game
//! stays one code path.

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

/// Turn `+name alice +exec x.cfg` style arguments into console lines.
/// (Native only, but tested everywhere, like [`query_lines`].)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn plus_lines(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for arg in args {
        if let Some(name) = arg.strip_prefix('+') {
            if !current.is_empty() {
                lines.push(current.join(" "));
            }
            current = vec![name.to_string()];
        } else if !current.is_empty() {
            current.push(arg);
        }
    }
    if !current.is_empty() {
        lines.push(current.join(" "));
    }
    lines
}

/// Every `c` value in a URL query (`?c=exec%20cfg/bots.cfg&c=sky+day`),
/// decoded: `%XX` escapes, and `+` as a space. (Browser only.)
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn query_lines(query: &str) -> Vec<String> {
    query
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.strip_prefix("c="))
        .map(url_decode)
        .filter(|line| !line.trim().is_empty())
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn url_decode(text: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(high), Some(low)) => {
                    out.push(high * 16 + low);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::path::PathBuf;

    use crate::config::GAME_NAME;

    /// `$XDG_CONFIG_HOME/<game name>/config.cfg`, falling back to `~/.config`.
    fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join(GAME_NAME).join("config.cfg"))
    }

    /// The saved settings, as console lines, if there are any.
    pub fn load_settings() -> Option<String> {
        std::fs::read_to_string(config_path()?).ok()
    }

    pub fn save_settings(text: &str) -> Result<(), String> {
        let path = config_path().ok_or("no config folder (HOME is not set)")?;
        path.parent()
            .map(std::fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|()| std::fs::write(&path, text))
            .map_err(|e| format!("could not save {}: {e}", path.display()))
    }

    /// Console lines from the command line: `+name alice +host`.
    pub fn launch_lines() -> Vec<String> {
        super::plus_lines(std::env::args().skip(1))
    }

    /// Read a script for `exec`. Natively the answer is immediate.
    pub fn read_script(path: &str, done: impl FnOnce(Result<String, String>) + Send + 'static) {
        done(std::fs::read_to_string(path).map_err(|e| e.to_string()));
    }

    /// Natively Bevy's own cursor state is right.
    pub fn pointer_locked() -> Option<bool> {
        None
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    /// localStorage key for the settings (the same lines as `config.cfg`).
    const KEY: &str = "fast-fps.config";

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn load_settings() -> Option<String> {
        storage()?.get_item(KEY).ok()?
    }

    pub fn save_settings(text: &str) -> Result<(), String> {
        storage()
            .ok_or("this browser won't store settings")?
            .set_item(KEY, text)
            .map_err(|_| "this browser won't store settings".to_string())
    }

    /// Console lines from the page's URL: `?c=exec%20cfg/bots.cfg&c=sky+day`.
    pub fn launch_lines() -> Vec<String> {
        let query = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        super::query_lines(&query)
    }

    /// Fetch a script for `exec` from the server, relative to the page. The
    /// answer comes back later, through `done`.
    pub fn read_script(path: &str, done: impl FnOnce(Result<String, String>) + Send + 'static) {
        ehttp::fetch(ehttp::Request::get(path), move |result| {
            done(match result {
                Ok(response) if response.ok => response
                    .text()
                    .map(str::to_string)
                    .ok_or_else(|| "not text".to_string()),
                Ok(response) => Err(format!("{} {}", response.status, response.status_text)),
                Err(e) => Err(e),
            });
        });
    }

    /// Whether the page holds the mouse. The browser lets go of it on Esc
    /// without the game hearing about it, so the game asks.
    pub fn pointer_locked() -> Option<bool> {
        let document = web_sys::window()?.document()?;
        Some(document.pointer_lock_element().is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_arguments() {
        let args = "+name alice +exec a.cfg +host".split(' ').map(String::from);
        assert_eq!(plus_lines(args), ["name alice", "exec a.cfg", "host"]);
        assert!(plus_lines(["stray".to_string()]).is_empty());
    }

    #[test]
    fn url_queries() {
        assert_eq!(
            query_lines("?c=exec%20cfg/bots.cfg&x=1&c=sky+day"),
            ["exec cfg/bots.cfg", "sky day"]
        );
        assert_eq!(query_lines("?c=fov%2"), ["fov%2"]);
        assert!(query_lines("").is_empty());
        assert_eq!(query_lines("c=%7E"), ["~"]);
    }
}
