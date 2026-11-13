// SPDX-License-Identifier: GPL-3.0-or-later
//! Original SetSimView decision/math boundary. No selected Sim, source tuning
//! selector, actor quaternion/world basis, native update cadence or data write.

pub type Result<T> = std::result::Result<T, String>;
pub const SOURCE_HOLD_DELAY_MS: u64 = 300;

#[derive(Clone, Copy, Debug)]
pub struct SourceCasRotationTuning {
    pub view_angles: [f32; 6],
    pub lerp_speed_mult: f32,
    pub rotate_key_speed: f32,
    pub rotate_key_speed_fast: f32,
    pub rotate_continuous_speed: f32,
    pub rotate_continuous_ease_in: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceCasViewTarget {
    pub preset_index: usize,
    pub angle_degrees: f32,
    /// Native quaternion interpolation progress rate, not degrees per second.
    pub progress_rate: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SourceCasViewAction {
    IncrementDegrees(f32),
    PresetTarget(SourceCasViewTarget),
}
#[derive(Clone, Copy, Debug)]
struct Press {
    left: bool,
    deadline_ms: u64,
    hold_started: bool,
    accumulated_speed: f32,
}
#[derive(Clone, Debug, Default)]
pub struct SourceCasViewController {
    press: Option<Press>,
}
impl SourceCasViewController {
    /// Native press replaces the pending direction, callbacks and deadline.
    pub fn press(&mut self, left: bool, monotonic_ms: u64) -> Result<()> {
        let deadline_ms = monotonic_ms
            .checked_add(SOURCE_HOLD_DELAY_MS)
            .ok_or("Source CAS view clock exceeds bounded deadline domain")?;
        self.press = Some(Press {
            left,
            deadline_ms,
            hold_started: false,
            accumulated_speed: 0.0,
        });
        Ok(())
    }
    /// Invoke once per explicitly selected source update. This does not choose
    /// a render FPS, synthesize elapsed-time substeps or invent degrees/second.
    /// Native starts holding strictly AFTER the deadline, not at equality.
    pub fn tick(
        &mut self,
        monotonic_ms: u64,
        tuning: &SourceCasRotationTuning,
    ) -> Result<Option<SourceCasViewAction>> {
        let Some(mut press) = self.press else {
            return Ok(None);
        };
        if monotonic_ms < press.deadline_ms - SOURCE_HOLD_DELAY_MS {
            return Err(
                "Source CAS view monotonic clock moved backwards; pending state retained".into(),
            );
        }
        tuning.validate()?;
        if !press.hold_started {
            if monotonic_ms <= press.deadline_ms {
                return Ok(None);
            }
            press.hold_started = true;
            press.accumulated_speed = 0.0;
        }
        let speed = press.accumulated_speed + tuning.rotate_continuous_ease_in;
        finite([speed])?;
        // MINSS(cap, speed) selects its second operand on equality, including
        // the original signed-zero behavior.
        press.accumulated_speed = if tuning.rotate_continuous_speed < speed {
            tuning.rotate_continuous_speed
        } else {
            speed
        };
        let signed = press.accumulated_speed * if press.left { 1.0 } else { -1.0 };
        let degrees = signed * f32::from_bits(0x3e19_999a);
        finite([degrees])?;
        self.press = Some(press);
        Ok(Some(SourceCasViewAction::IncrementDegrees(degrees)))
    }
    /// A release uses the recorded hold transition. Merely waiting longer
    /// than300ms without a source tick does not manufacture a held event.
    pub fn release(
        &mut self,
        source_heading_degrees: f32,
        tuning: &SourceCasRotationTuning,
    ) -> Result<Option<SourceCasViewAction>> {
        let Some(press) = self.press else {
            return Ok(None);
        };
        let action = if press.hold_started {
            None
        } else {
            Some(SourceCasViewAction::PresetTarget(source_cas_view_target(
                source_heading_degrees,
                press.left,
                tuning,
            )?))
        };
        self.press = None;
        Ok(action)
    }
    /// Explicit cancellation clears a pending input without applying a preset.
    pub fn cancel(&mut self) {
        self.press = None;
    }
    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }
    pub fn hold_started(&self) -> bool {
        self.press.is_some_and(|p| p.hold_started)
    }
    pub fn accumulated_speed(&self) -> Option<f32> {
        self.press.map(|p| p.accumulated_speed)
    }
}
impl SourceCasRotationTuning {
    pub fn validate(&self) -> Result<()> {
        finite(self.view_angles.into_iter().chain([
            self.lerp_speed_mult,
            self.rotate_key_speed,
            self.rotate_key_speed_fast,
            self.rotate_continuous_speed,
            self.rotate_continuous_ease_in,
        ]))
    }
}
/// Numeric preset loop of00455AD0 (cold block004897AB..0048987F).
/// Heading must be the resolved source heading; this does not derive it from
/// an arbitrary host yaw or sort/rewrite the six source preset angles.
pub fn source_cas_next_preset_index(heading: f32, left: bool, angles: [f32; 6]) -> Result<usize> {
    finite(angles.into_iter().chain([heading]))?;
    let epsilon = f32::from_bits(0x3c23_d70a);
    for index in 0..6 {
        let next = (index + 1) % 6;
        let first = angles[index];
        let second = angles[next];
        if (heading - first).abs() <= epsilon {
            return Ok(if left { next } else { (index + 5) % 6 });
        }
        let after_first = first < heading - epsilon;
        let between = if second <= first {
            after_first
        } else {
            after_first && heading + epsilon < second
        };
        if between {
            return Ok(if left { next } else { index });
        }
    }
    Ok(0)
}
pub fn source_cas_view_target(
    heading: f32,
    left: bool,
    tuning: &SourceCasRotationTuning,
) -> Result<SourceCasViewTarget> {
    tuning.validate()?;
    let index = source_cas_next_preset_index(heading, left, tuning.view_angles)?;
    let angle = tuning.view_angles[index];
    let mut difference = (heading - angle).abs();
    if difference > 180.0 {
        difference = 360.0 - difference;
    }
    let progress_rate = (tuning.lerp_speed_mult * 180.0) / (difference + f32::EPSILON);
    finite([progress_rate])?;
    Ok(SourceCasViewTarget {
        preset_index: index,
        angle_degrees: angle,
        progress_rate,
    })
}
fn finite(values: impl IntoIterator<Item = f32>) -> Result<()> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err("Unqualified non-finite source CAS rotation input/result; source and pending state retained".into())
    }
}
