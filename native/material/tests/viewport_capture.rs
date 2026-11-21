use external_cas_material::viewport_capture::{CaptureError, CaptureReceipt, write_new_capture};
use std::fs;

const PIXELS: [u8; 8] = [255, 17, 31, 255, 31, 166, 97, 110];
const OTHER: [u8; 8] = [31, 171, 201, 255, 21, 53, 201, 251];

#[test]
fn publishes_actual_png_in_chosen_folder_and_verifies_rgba_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("capture β.png");
    assert_eq!(
        write_new_capture(&file, &PIXELS, 2, 1).unwrap(),
        CaptureReceipt {
            width: 2,
            height: 1
        }
    );
    let decoded = image::open(&file).unwrap().into_rgba8();
    assert_eq!(decoded.dimensions(), (2, 1));
    assert_eq!(&decoded.into_raw(), &PIXELS);
    assert_eq!(dir.path().read_dir().unwrap().count(), 1);
}

#[test]
fn prior_capture_and_existing_file_of_any_contents_are_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("previous.png");
    write_new_capture(&file, &PIXELS, 2, 1).unwrap();
    let baseline = fs::read(&file).unwrap();
    assert_eq!(
        write_new_capture(&file, &OTHER, 2, 1).unwrap_err().code(),
        "destination_exists"
    );
    assert_eq!(fs::read(&file).unwrap(), baseline);
    let sentinel = dir.path().join("diagnostic.png");
    fs::write(&sentinel, b"manual evidence").unwrap();
    assert!(matches!(
        write_new_capture(&sentinel, &PIXELS, 2, 1),
        Err(CaptureError::DestinationExists)
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"manual evidence");
}

#[test]
fn contending_captures_leave_exactly_one_complete_file_without_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("same.png");
    let start = std::sync::Barrier::new(2);
    let (first, second) = std::thread::scope(|s| {
        let first = s.spawn(|| {
            start.wait();
            write_new_capture(&file, &PIXELS, 2, 1)
        });
        let second = s.spawn(|| {
            start.wait();
            write_new_capture(&file, &OTHER, 2, 1)
        });
        (first.join().unwrap(), second.join().unwrap())
    });
    let chosen = match (first, second) {
        (Ok(_), Err(CaptureError::DestinationExists)) => PIXELS,
        (Err(CaptureError::DestinationExists), Ok(_)) => OTHER,
        outcomes => panic!("Expected one winner and one refusal: {outcomes:?}"),
    };
    assert_eq!(image::open(&file).unwrap().into_rgba8().into_raw(), chosen);
    assert_eq!(dir.path().read_dir().unwrap().count(), 1);
}

#[test]
fn invalid_pixels_and_encoder_failure_leave_no_output_or_staging_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("frame.png");
    assert!(matches!(
        write_new_capture(&file, &PIXELS, 3, 1),
        Err(CaptureError::InvalidPixels)
    ));
    assert!(matches!(
        write_new_capture(&file, &[], 0, 0),
        Err(CaptureError::InvalidPixels)
    ));
    assert!(matches!(
        write_new_capture(&file, &PIXELS, u32::MAX, u32::MAX),
        Err(CaptureError::InvalidPixels)
    ));
    // .dds is a known image format, but this production image build has no
    // DDS writer, so this specifically exercises failure after staging.
    let unsupported = dir.path().join("cannot-encode.dds");
    assert!(matches!(
        write_new_capture(&unsupported, &PIXELS, 2, 1),
        Err(CaptureError::Encode(_))
    ));
    assert_eq!(dir.path().read_dir().unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn existing_symlink_or_dangling_output_is_not_replaced_or_followed() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("notes");
    let target = dir.path().join("linked.png");
    fs::write(&original, b"keep this").unwrap();
    symlink(&original, &target).unwrap();
    assert!(matches!(
        write_new_capture(&target, &PIXELS, 2, 1),
        Err(CaptureError::DestinationExists)
    ));
    let missing = dir.path().join("not-created");
    let dangling = dir.path().join("dangling.png");
    symlink(&missing, &dangling).unwrap();
    assert!(matches!(
        write_new_capture(&dangling, &PIXELS, 2, 1),
        Err(CaptureError::DestinationExists)
    ));
    assert_eq!(fs::read(&original).unwrap(), b"keep this");
    assert_eq!(fs::read_link(&target).unwrap(), original);
    assert_eq!(fs::read_link(&dangling).unwrap(), missing);
    assert!(!missing.exists());
}
