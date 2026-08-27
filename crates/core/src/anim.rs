//! Easing curves and keyframe sampling

use crate::doc::{Common, Ease, Key, KeyValue, Prop};
use crate::scalar::Scalar;

/// Apply easing function to normalized progress [0, 1].
pub fn ease(e: Ease, x: f64) -> f64 {
    match e {
        Ease::Linear => x,
        Ease::InCubic => x * x * x,
        Ease::OutCubic => 1.0 - (1.0 - x).powi(3),
        Ease::InOutCubic => {
            if x < 0.5 {
                4.0 * x * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
            }
        }
    }
}

/// Resolved animation values at a point in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    pub translate: (f64, f64),
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
}

/// Sample a keyframe track at time t, returning an owned KeyValue.
fn sample(keys: &[Key], t: i64) -> KeyValue {
    debug_assert!(!keys.is_empty(), "sample called on empty track");

    // Before first key: return first value
    if t <= keys[0].t {
        return keys[0].v.clone();
    }

    // After last key: return last value
    if t >= keys[keys.len() - 1].t {
        return keys[keys.len() - 1].v.clone();
    }

    // Find the segment (k0, k1) where k0.t < t < k1.t
    let mut k0_idx = 0;
    for (i, key) in keys.iter().enumerate() {
        if key.t < t {
            k0_idx = i;
        } else {
            break;
        }
    }

    let k0 = &keys[k0_idx];
    let k1 = &keys[k0_idx + 1];

    // Normalize progress in [0, 1]
    let duration = (k1.t - k0.t) as f64;
    let elapsed = (t - k0.t) as f64;
    let u = elapsed / duration;

    // Apply easing from the key being entered (k1.ease)
    let w = ease(k1.ease, u);

    // Interpolate based on value type
    match (&k0.v, &k1.v) {
        (KeyValue::Num(v0), KeyValue::Num(v1)) => {
            let interp = v0.0 + (v1.0 - v0.0) * w;
            KeyValue::Num(Scalar(interp))
        }
        (KeyValue::Vec2(v0), KeyValue::Vec2(v1)) => {
            let x = v0[0].0 + (v1[0].0 - v0[0].0) * w;
            let y = v0[1].0 + (v1[1].0 - v0[1].0) * w;
            KeyValue::Vec2([Scalar(x), Scalar(y)])
        }
        _ => {
            // Arity mismatch (shouldn't happen after validation)
            k0.v.clone()
        }
    }
}

/// Resolve Common animation values at time t.
/// Starts from static defaults, overridden by static statics, then tracks.
pub fn resolve_common(c: &Common, t: i64) -> Resolved {
    // Start with defaults
    let mut result = Resolved {
        translate: (0.0, 0.0),
        scale: 1.0,
        rotation: 0.0,
        opacity: 1.0,
    };

    // Override with static values
    if let Some(tr) = c.translate {
        result.translate = (tr[0].0, tr[1].0);
    }
    if let Some(sc) = c.scale {
        result.scale = sc.0;
    }
    if let Some(rot) = c.rotation {
        result.rotation = rot.0;
    }
    if let Some(op) = c.opacity {
        result.opacity = op.0;
    }

    // Override with track values
    for track in &c.animations {
        let sampled = sample(&track.keys, t);
        match track.prop {
            Prop::Translate => {
                if let KeyValue::Vec2(v) = sampled {
                    result.translate = (v[0].0, v[1].0);
                }
            }
            Prop::Scale => {
                if let KeyValue::Num(v) = sampled {
                    result.scale = v.0;
                }
            }
            Prop::Rotation => {
                if let KeyValue::Num(v) = sampled {
                    result.rotation = v.0;
                }
            }
            Prop::Opacity => {
                if let KeyValue::Num(v) = sampled {
                    result.opacity = v.0;
                }
            }
        }
    }

    result
}
