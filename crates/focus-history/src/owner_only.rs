#[cfg(unix)]
pub(crate) use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

#[cfg(not(unix))]
pub(crate) trait DirBuilderExt {
    fn mode(&mut self, mode: u32) -> &mut Self;
}

#[cfg(not(unix))]
impl DirBuilderExt for std::fs::DirBuilder {
    fn mode(&mut self, _: u32) -> &mut Self {
        self
    }
}

#[cfg(not(unix))]
pub(crate) trait OpenOptionsExt {
    fn mode(&mut self, mode: u32) -> &mut Self;
}

#[cfg(not(unix))]
impl OpenOptionsExt for std::fs::OpenOptions {
    fn mode(&mut self, _: u32) -> &mut Self {
        self
    }
}
