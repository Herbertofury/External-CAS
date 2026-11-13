// SPDX-License-Identifier: GPL-3.0-or-later
//! Original source lifecycle only. No replacement panels or fabricated data.
use crate::{HostValue, SourceUiRuntime};

#[derive(Clone, Debug, Default)]
pub struct BootstrapStatus {
    pub definitions_ready: bool,
    pub localization_bound: bool,
    pub source_root_ready: bool,
    pub widget_initialized: bool,
    pub errors: Vec<String>,
}

pub fn bind_localization(runtime: &mut SourceUiRuntime) -> Result<(), String> {
    runtime.bind_native_function_projected(
        "olympus.localization.LocKey",
        "sGetLocalizedStringFunc",
        "ExternalCAS.GetLocalizedString",
        &["hash", "tokens", "flags"],
    )
}

/// UIContainer initializes its own original managers. A standalone widget uses
/// its actual wrapper.widget; it must never be confused with the wrapper root.
pub fn initialize_widget(runtime: &mut SourceUiRuntime, name: &str) -> Result<u64, String> {
    try_initialize_widget(runtime, name)?
        .ok_or_else(|| "Original movie wrapper/widget is not ready".into())
}
/// Waiting for the genuine frame-script-created widget is not an initialization
/// error. Once Initialize is invoked, its error is retained and never retried.
pub fn try_initialize_widget(
    runtime: &mut SourceUiRuntime,
    name: &str,
) -> Result<Option<u64>, String> {
    let root = match runtime.retain_source_root() {
        Ok(HostValue::Avm2Handle(root)) => root,
        Ok(HostValue::Null | HostValue::Undefined) => return Ok(None),
        Err(e) if e.contains("not ready") => return Ok(None),
        _ => return Err("Original source root is not an object".into()),
    };
    let stage = match runtime.get_source_property(root, "stage")? {
        HostValue::Avm2Handle(stage) => stage,
        HostValue::Null | HostValue::Undefined => return Ok(None),
        _ => return Err("Original source stage is not an object".into()),
    };
    let widget = match runtime.get_source_property(root, "widget")? {
        HostValue::Avm2Handle(widget) => widget,
        HostValue::Null | HostValue::Undefined => return Ok(None),
        _ => return Err("Original movie wrapper has no source widget".into()),
    };
    runtime.set_source_property(stage, "scaleMode", HostValue::String("noScale".into()))?;
    runtime.set_source_property(stage, "align", HostValue::String("TL".into()))?;
    runtime.set_source_property(widget, "WidgetName", HostValue::String(name.into()))?;
    runtime.call_source_method(widget, "Initialize", vec![])?;
    Ok(Some(widget))
}
