// SPDX-License-Identifier: GPL-3.0-or-later
//! Own-source fixtures only. No installed movie or proprietary font/art bytes.
use ruffle_core::{
    Player, PlayerBuilder,
    backend::log::LogBackend,
    context::UpdateContext,
    external::{ExternalInterfaceProvider, Value},
    limits::ExecutionLimit,
    tag_utils::SwfMovieData,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);
impl LogBackend for Log {
    fn avm_trace(&self, s: &str) {
        self.0.lock().unwrap().push(s.into());
    }
    fn avm_warning(&self, s: &str) {
        self.0.lock().unwrap().push(format!("warning:{s}"));
    }
    fn avm_error(&self, s: &str) {
        self.0.lock().unwrap().push(format!("error:{s}"));
    }
}
#[derive(Clone, Default)]
struct Host(Arc<Mutex<Vec<Value>>>);
impl ExternalInterfaceProvider for Host {
    fn call_method(&self, _: &mut UpdateContext<'_>, name: &str, args: &[Value]) -> Value {
        self.0.lock().unwrap().extend_from_slice(args);
        if name == "FixtureNative" {
            if let Some(Value::Object(input)) = args.first() {
                if let Some(Value::Number(hash)) = input.get("hash") {
                    return Value::Number(hash + 7.);
                }
            }
        }
        Value::Undefined
    }
    fn on_callback_available(&self, _: &str) {}
    fn get_id(&self) -> Option<String> {
        None
    }
}
fn run(bytes: &[u8]) -> (Arc<Mutex<Player>>, Log, Host) {
    let log = Log::default();
    let host = Host::default();
    let m = SwfMovieData::from_data(
        bytes,
        "https://source-ui.invalid/own-fixture.swf".into(),
        None,
        None,
    )
    .unwrap();
    let p = PlayerBuilder::new()
        .with_movie(m)
        .with_log(log.clone())
        .with_external_interface(Box::new(host.clone()))
        .with_autoplay(true)
        .build();
    for _ in 0..3 {
        let mut p = p.lock().unwrap();
        p.preload(&mut ExecutionLimit::with_max_ops_and_time(
            10000,
            Duration::from_millis(20),
        ));
        p.run_frame();
    }
    (p, log, host)
}
#[test]
fn original_class_only_placement_uses_symbol_parent_and_name_once() {
    let (_, log, _) = run(include_bytes!("fixtures/class-only-placement.swf"));
    assert_eq!(
        *log.0.lock().unwrap(),
        [
            "class-only-child namedChild true",
            "class-only-root true namedChild"
        ]
    );
}
#[test]
fn instance_class_override_preserves_source_symbol_and_does_not_leak_to_other_instances() {
    let (_, log, _) = run(include_bytes!("fixtures/instance-class-override.swf"));
    let lines = log.0.lock().unwrap();
    assert_eq!(
        *lines,
        [
            "class-only-child namedChild true",
            "instance-override true",
            "class-only-child namedBase true",
            "override-root true false"
        ]
    );
}
#[test]
fn source_function_roundtrips_as_rooted_callback_and_cross_runtime_handle_is_refused() {
    let bytes = include_bytes!("fixtures/HostCallbackFixture.swf");
    let (a, _, ha) = run(bytes);
    let (b, _, hb) = run(bytes);
    let first = ha.0.lock().unwrap()[0].clone();
    let second = hb.0.lock().unwrap()[0].clone();
    let Value::Avm2Handle(id) = first else {
        panic!("original VM callback was lost instead of rooted")
    };
    assert_ne!(first, second, "VM handles must not alias between runtimes");
    assert_eq!(
        a.lock()
            .unwrap()
            .call_retained_avm2_function(id, vec![Value::Number(35.)])
            .unwrap(),
        Value::Number(42.)
    );
    assert!(
        b.lock()
            .unwrap()
            .call_retained_avm2_function(id, vec![])
            .is_err()
    );
}

#[test]
fn original_function_slot_invokes_native_host_with_unchanged_argument_object() {
    use std::collections::BTreeMap;
    let (p, log, host) = run(include_bytes!("fixtures/native-function-slot.swf"));
    let mut p = p.lock().unwrap();
    p.bind_avm2_static_host_function("NativeBindingTarget", "nativeSlot", "FixtureNative").unwrap();
    let input = Value::Object(BTreeMap::from([("hash".into(), Value::Number(35.))]));
    assert_eq!(p.call_internal_interface("InvokeNative", [input.clone()]), Value::Number(42.));
    assert_eq!(host.0.lock().unwrap().last(), Some(&input));
    let Value::Avm2Handle(root)=p.retain_avm2_root_object().unwrap() else {panic!("root identity lost")};
    assert_eq!(p.call_retained_avm2_method(root,"invokeNative",vec![input.clone()]).unwrap(),Value::Number(42.));
    let Value::Avm2Handle(callback)=p.get_retained_avm2_property(root,"invokeNative").unwrap() else {panic!("source method identity lost")};
    assert_eq!(p.call_retained_avm2_function(callback,vec![input]).unwrap(),Value::Number(42.));
    assert!(matches!(p.construct_avm2_script_class("NativeBindingTarget",vec![]).unwrap(),Value::Avm2Handle(_)));
    assert!(p.call_retained_avm2_method(root,"missing",vec![]).is_err());

    assert!(p.bind_avm2_static_host_function("NativeBindingTarget", "nativeSlot", "OtherBridge").is_err());
    assert!(p.bind_avm2_static_host_function("NativeBindingTarget", "missing", "FixtureNative").is_err());
    assert!(log.0.lock().unwrap().iter().all(|v| !v.starts_with("error:")), "{:?}", log.0.lock().unwrap());
}

#[test]
fn native_argument_projection_reads_declared_source_slots_without_vm_reentry() {
    use std::collections::BTreeMap;
    let (p, log, host)=run(include_bytes!("fixtures/NativeProjectionFixture.swf"));
    let mut p=p.lock().unwrap();
    p.bind_avm2_static_host_function_projected("NativeProjectionFixture","nativeSlot","FixtureNative",&["hash","tokens","flags"]).unwrap();
    assert_eq!(p.call_internal_interface("InvokeNativeProjection",[]),Value::Number(42.));
    assert_eq!(host.0.lock().unwrap().last(),Some(&Value::Object(BTreeMap::from([
        ("hash".into(),Value::Number(35.)),("tokens".into(),Value::Null),("flags".into(),Value::Number(0.))]))));
    let errors=log.0.lock().unwrap().clone();assert!(errors.iter().all(|v| !v.starts_with("error:")),"{errors:?}");
}
