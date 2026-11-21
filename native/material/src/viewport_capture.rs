//! Encode a diagnostic capture into its chosen directory, publishing only a
//! complete new image. Never open an existing destination for writing.
use std::{fmt, io, path::Path};

#[derive(Debug)]
pub enum CaptureError {
    InvalidPixels,
    DestinationExists,
    Io(io::Error),
    Encode(image::ImageError),
}

impl CaptureError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidPixels => "invalid_pixels",
            Self::DestinationExists => "destination_exists",
            Self::Io(_) => "io_error",
            Self::Encode(_) => "encode_error",
        }
    }
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPixels => write!(f, "Invalid screenshot dimensions or pixel length."),
            Self::DestinationExists => write!(
                f,
                "That output already exists. Choose a new capture filename."
            ),
            Self::Io(err) => write!(f, "{err}"),
            Self::Encode(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CaptureError {}

#[derive(Debug, PartialEq, Eq)]
pub struct CaptureReceipt {
    pub width: u32,
    pub height: u32,
}

pub fn write_new_capture(
    path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<CaptureReceipt, CaptureError> {
    let expected = usize::try_from(width)
        .ok()
        .zip(usize::try_from(height).ok())
        .and_then(|(w, h)| w.checked_mul(h))
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(rgba.len()) {
        return Err(CaptureError::InvalidPixels);
    }
    // Fail fast for a previously saved image or other object. This is only an
    // optimization; persist_noclobber also checks atomically where supported.
    match path.symlink_metadata() {
        Ok(_) => return Err(CaptureError::DestinationExists),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(CaptureError::Io(error)),
    }
    let format = image::ImageFormat::from_path(path).map_err(CaptureError::Encode)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut stage = tempfile::Builder::new()
        .prefix(".external-cas-capture-")
        .tempfile_in(parent)
        .map_err(CaptureError::Io)?;
    image::write_buffer_with_format(
        stage.as_file_mut(),
        rgba,
        width,
        height,
        image::ColorType::Rgba8,
        format,
    )
    .map_err(CaptureError::Encode)?;
    stage.as_file().sync_all().map_err(CaptureError::Io)?;
    stage.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            CaptureError::DestinationExists
        } else {
            CaptureError::Io(error.error)
        }
    })?;
    Ok(CaptureReceipt { width, height })
}
