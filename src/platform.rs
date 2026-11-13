//! OS-specific bits: Japanese fonts, file manager integration and UI wording.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Font files with Japanese glyphs, in order of preference.
#[cfg(windows)]
const FONT_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\meiryo.ttc",
    r"C:\Windows\Fonts\YuGothM.ttc",
    r"C:\Windows\Fonts\msgothic.ttc",
];

/// Font files with Japanese glyphs, in order of preference.
#[cfg(target_os = "macos")]
const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
];

/// Font files with Japanese glyphs, in order of preference.
#[cfg(all(unix, not(target_os = "macos")))]
const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
    "/usr/share/fonts/OTF/ipag.ttf",
    "/usr/share/fonts/truetype/takao-gothic/TakaoGothic.ttf",
    "/usr/share/fonts/truetype/vlgothic/VL-Gothic-Regular.ttf",
];

/// Bytes of the first available Japanese font.
pub fn japanese_font() -> Option<Vec<u8>> {
    if let Some(b) = FONT_CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) {
        return Some(b);
    }
    // Unknown distro layout: ask fontconfig.
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Ok(out) = Command::new("fc-match").args(["-f", "%{file}", "sans-serif:lang=ja"]).output() {
        if let Ok(path) = String::from_utf8(out.stdout) {
            return std::fs::read(path.trim()).ok();
        }
    }
    None
}

/// Context menu label for [`reveal`].
pub const REVEAL_LABEL: &str = if cfg!(windows) {
    "エクスプローラーで選択"
} else if cfg!(target_os = "macos") {
    "Finder で表示"
} else {
    "ファイルマネージャーで表示"
};

/// Tooltip explaining the inaccessible-folder count.
pub const ACCESS_HINT: &str = if cfg!(windows) {
    "権限がない等の理由で読み取れなかったフォルダ数です。\n管理者として実行すると減らせます。"
} else if cfg!(target_os = "macos") {
    "権限がない等の理由で読み取れなかったフォルダ数です。\nシステム設定 > プライバシーとセキュリティ > フルディスクアクセス で許可すると減らせます。"
} else {
    "権限がない等の理由で読み取れなかったフォルダ数です。\nroot 権限 (sudo) で実行すると減らせます。"
};

/// Turns user input into a path to scan (`C:` → `C:\` on Windows).
pub fn normalize_input(s: &str) -> PathBuf {
    let s = s.trim().trim_matches('"');
    if cfg!(windows) && s.len() == 2 && s.ends_with(':') {
        PathBuf::from(format!("{s}\\"))
    } else {
        PathBuf::from(s)
    }
}

/// Opens a folder in the file manager.
pub fn open_dir(path: &Path) {
    let program = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program).arg(path).spawn();
}

/// Shows the file manager with `path` selected.
#[cfg(windows)]
pub fn reveal(path: &Path) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

/// Shows the file manager with `path` selected.
#[cfg(target_os = "macos")]
pub fn reveal(path: &Path) {
    let _ = Command::new("open").arg("-R").arg(path).spawn();
}

/// Shows the file manager with `path` selected.
///
/// Uses the freedesktop FileManager1 D-Bus interface (Nautilus, Dolphin, Nemo, …);
/// if that is unavailable, just opens the parent folder.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn reveal(path: &Path) {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let shown = Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("array:string:{}", file_uri(&path)))
            .arg("string:")
            .output()
            .is_ok_and(|o| o.status.success());
        if !shown {
            open_dir(path.parent().unwrap_or(&path));
        }
    });
}

/// `file://` URI with everything but unreserved characters and `/` percent-encoded.
#[cfg(all(unix, not(target_os = "macos")))]
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut s = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}
