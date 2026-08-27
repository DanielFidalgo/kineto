use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// f64 that serializes as a JSON integer when integral (|v| < 2^53),
/// else as shortest-round-trip float. Matches JS JSON.stringify, which is
/// what makes cross-SDK byte identity possible (§3.7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scalar(pub f64);

impl Serialize for Scalar {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0.fract() == 0.0 && self.0.abs() < 9_007_199_254_740_992.0 {
            s.serialize_i64(self.0 as i64)
        } else {
            s.serialize_f64(self.0)
        }
    }
}
impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Scalar(f64::deserialize(d)?))
    }
}
impl From<f64> for Scalar {
    fn from(v: f64) -> Self {
        Scalar(v)
    }
}
