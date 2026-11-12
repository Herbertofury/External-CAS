use external_cas_material::{color::{Axis,PackedColorShift,SliderRange,shift_reference_rgb},Pixels,skin::soft_light_reference};
use std::sync::atomic::AtomicBool;
#[test]
fn every_raw_lane_roundtrips_without_touching_other_native_bits(){
    for axis in[Axis::Brightness,Axis::Saturation,Axis::Hue,Axis::Opacity]{
        let original=PackedColorShift(0x8123_4567_89ab_cdef);
        for raw in 0..=u16::MAX{let value=raw as i16;let changed=original.replace_lane(axis,value);assert_eq!(changed.raw_lane(axis),value);assert_eq!(changed.replace_lane(axis,original.raw_lane(axis)),original);}
    }
}
#[test]
fn explicit_quantization_and_fallback_ranges_preserve_serialized_values(){
    assert_eq!(PackedColorShift::default().hue_saturation_value_opacity(),[0.,0.,0.,1.]);
    let original=PackedColorShift(0xffff_9abc_8765_4321);
    for axis in[Axis::Brightness,Axis::Saturation,Axis::Hue,Axis::Opacity]{
        assert_eq!(original.replace_value(axis,original.value(axis)).unwrap(),original);
        assert!(SliderRange::tyjokr_fallback(axis).valid());
        assert!(original.replace_value(axis,f32::NAN).is_err());
        assert!(original.replace_value(axis,2.).is_err());
    }
    assert_eq!(original.replace_value(Axis::Hue,0.5).unwrap().raw_lane(Axis::Hue),8192);
}
#[test]
fn hue_wrap_saturation_value_and_neutral_identity(){
    let neutral=PackedColorShift::NEUTRAL;
    for rgb in [[0,0,0],[255,255,255],[17,93,226],[254,1,129]]{assert_eq!(shift_reference_rgb(rgb,neutral).unwrap(),rgb);}
    let shifted=neutral.replace_value(Axis::Hue,0.5).unwrap();assert_eq!(shift_reference_rgb([255,0,0],shifted).unwrap(),[0,255,255]);
    let shifted=neutral.replace_value(Axis::Saturation,-1.).unwrap();assert_eq!(shift_reference_rgb([0,255,0],shifted).unwrap(),[255,255,255]);
    assert!(shift_reference_rgb([10,20,30],PackedColorShift(0)).is_ok());
    assert!(shift_reference_rgb([10,20,30],PackedColorShift(u64::MAX)).is_err());
}
#[test]
fn skin_reference_preserves_inputs_and_refuses_unqualified_planes(){
    let tone=Pixels::new(1,1,vec![90,60,40,255],1024).unwrap();let detail=Pixels::new(1,1,vec![180,180,180,255],1024).unwrap();
    let out=soft_light_reference(&tone,&detail,0.35,1024,&AtomicBool::new(false)).unwrap();assert_eq!(out.rgba[3],255);assert_eq!(tone.rgba,[90,60,40,255]);
    assert!(soft_light_reference(&tone,&detail,0.35,1024,&AtomicBool::new(true)).is_err());
    assert!(soft_light_reference(&tone,&detail,f64::NAN,1024,&AtomicBool::new(false)).is_err());
    let transparent=Pixels::new(1,1,vec![90,60,40,0],1024).unwrap();assert!(soft_light_reference(&transparent,&detail,0.35,1024,&AtomicBool::new(false)).is_err());
}
