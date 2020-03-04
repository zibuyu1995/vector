// === Fun little hack around bytse and OsStr ===

#[derive(Debug, Clone)]
struct BytesPath {
    #[cfg(unix)]
    path: Bytes,
    #[cfg(windows)]
    path: path::PathBuf,
}

impl BytesPath {
    #[cfg(unix)]
    fn new(path: Bytes) -> BytesPath {
        BytesPath { path }
    }
    #[cfg(windows)]
    fn new(path: Bytes) -> BytesPath {
        let utf8_string = String::from_utf8_lossy(&path[..]);
        let path = path::PathBuf::from(utf8_string.as_ref());
        BytesPath { path }
    }
}

impl AsRef<std::path::Path> for BytesPath {
    #[cfg(unix)]
    fn as_ref(&self) -> &std::path::Path {
        use std::os::unix::ffi::OsStrExt;
        let os_str = std::ffi::OsStr::from_bytes(&self.path);
        &std::path::Path::new(os_str)
    }
    #[cfg(windows)]
    fn as_ref(&self) -> &std::path::Path {
        &self.path.as_ref()
    }
}
