//! SVG path data: parsing into absolute segments, and trimming by length.
//!
//! Pure geometry, no I/O. Everything the renderer needs from a `d` string
//! is reduced to four segment kinds so the rasterizer never sees a relative
//! coordinate, a quadratic or an arc. Trig goes through `libm` rather than
//! `f64::sin` and friends: the std versions defer to the platform's math
//! library, which differs between native and wasm and would break the
//! byte-identical parity gate.

use std::f64::consts::{FRAC_PI_2, PI};

use thiserror::Error;

/// Equal-`t` pieces a cubic is flattened into when measuring its length.
/// A fixed count rather than a tolerance, so the measured length (and thus
/// where a trim lands) cannot differ between targets.
pub const CUBIC_LENGTH_PIECES: usize = 32;

/// Slack when counting quarter turns, so an arc of exactly 180 degrees does
/// not become three segments because of a rounding error in `atan2`.
const ARC_SPLIT_EPSILON: f64 = 1e-9;

/// An absolute path segment. `Close` returns the pen to the subpath start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    MoveTo([f64; 2]),
    LineTo([f64; 2]),
    CubicTo([f64; 2], [f64; 2], [f64; 2]),
    Close,
}

/// A parse failure. `offset` is always the index of a byte that exists in
/// the input, so a caller can point at it.
#[derive(Clone, Debug, PartialEq, Error)]
#[error("at byte {offset}: {reason}")]
pub struct PathDataError {
    pub offset: usize,
    pub reason: String,
}

use Segment::{Close, CubicTo, LineTo, MoveTo};

/// Parse SVG path data (`d`) into absolute segments.
pub fn parse(d: &str) -> Result<Vec<Segment>, PathDataError> {
    let mut parser = Parser::new(d);
    parser.run()?;
    parser.finish()
}

/// A polyline as segments: `MoveTo`, `LineTo`s, and `Close` when `closed`.
pub fn from_points(points: &[[f64; 2]], closed: bool) -> Vec<Segment> {
    let Some((first, rest)) = points.split_first() else {
        return Vec::new();
    };
    let mut segments = vec![MoveTo(*first)];
    segments.extend(rest.iter().map(|&p| LineTo(p)));
    if closed {
        segments.push(Close);
    }
    segments
}

/// The first `fraction` of the path's total length, drawn by one pen: the
/// subpaths follow each other in order. The result is a valid path in its
/// own right; a cut curve is the true sub-curve, not a polyline.
///
/// A partly drawn closing edge becomes a `LineTo`, and `Close` is kept only
/// when the whole contour is drawn, so a half-drawn square has no closing
/// edge to flash into view.
pub fn trim(segments: &[Segment], fraction: f64) -> Vec<Segment> {
    if fraction.is_nan() || fraction <= 0.0 {
        return Vec::new();
    }
    if fraction >= 1.0 {
        return segments.to_vec();
    }
    let spans = measure(segments);
    let target = fraction * spans.iter().map(|span| span.length).sum::<f64>();

    let mut trimmed = Vec::new();
    let mut drawn = 0.0;
    for span in &spans {
        if drawn >= target {
            break;
        }
        if drawn + span.length <= target {
            trimmed.push(span.segment);
            drawn += span.length;
        } else {
            trimmed.push(span.cut_at(target - drawn));
            break;
        }
    }
    trimmed
}

// --- measuring and cutting -------------------------------------------------

/// A segment with the endpoints and length that trimming needs.
struct Span {
    segment: Segment,
    from: [f64; 2],
    to: [f64; 2],
    length: f64,
}

impl Span {
    /// The segment drawn up to `distance` along it, `0 < distance < length`.
    fn cut_at(&self, distance: f64) -> Segment {
        match self.segment {
            CubicTo(c1, c2, end) => {
                let t = cubic_parameter_at(self.from, c1, c2, end, distance);
                let (left_c1, left_c2, point) = split_cubic_left(self.from, c1, c2, end, t);
                CubicTo(left_c1, left_c2, point)
            }
            _ => LineTo(lerp(self.from, self.to, distance / self.length)),
        }
    }
}

fn measure(segments: &[Segment]) -> Vec<Span> {
    let mut cursor = [0.0, 0.0];
    let mut subpath_start = cursor;
    let mut spans = Vec::with_capacity(segments.len());
    for &segment in segments {
        let from = cursor;
        let (to, length) = match segment {
            MoveTo(p) => {
                subpath_start = p;
                (p, 0.0)
            }
            LineTo(p) => (p, distance(from, p)),
            CubicTo(c1, c2, p) => (p, cubic_length_table(from, c1, c2, p)[CUBIC_LENGTH_PIECES]),
            Close => (subpath_start, distance(from, subpath_start)),
        };
        cursor = to;
        spans.push(Span { segment, from, to, length });
    }
    spans
}

/// Cumulative chord length at `t = i / CUBIC_LENGTH_PIECES`; entry 0 is 0.
fn cubic_length_table(p0: [f64; 2], c1: [f64; 2], c2: [f64; 2], p1: [f64; 2]) -> [f64; CUBIC_LENGTH_PIECES + 1] {
    let mut table = [0.0; CUBIC_LENGTH_PIECES + 1];
    let mut previous = p0;
    for (i, entry) in table.iter_mut().enumerate().skip(1) {
        let t = i as f64 / CUBIC_LENGTH_PIECES as f64;
        let point = split_cubic_left(p0, c1, c2, p1, t).2;
        *entry = distance(previous, point);
        previous = point;
    }
    for i in 1..table.len() {
        table[i] += table[i - 1];
    }
    table
}

/// The curve parameter `distance` along the cubic, interpolating within the
/// flattened piece it falls in.
fn cubic_parameter_at(p0: [f64; 2], c1: [f64; 2], c2: [f64; 2], p1: [f64; 2], distance: f64) -> f64 {
    let table = cubic_length_table(p0, c1, c2, p1);
    let piece = (0..CUBIC_LENGTH_PIECES)
        .find(|&i| distance <= table[i + 1])
        .unwrap_or(CUBIC_LENGTH_PIECES - 1);
    let piece_length = table[piece + 1] - table[piece];
    let within_piece = if piece_length > 0.0 { (distance - table[piece]) / piece_length } else { 0.0 };
    (piece as f64 + within_piece.clamp(0.0, 1.0)) / CUBIC_LENGTH_PIECES as f64
}

/// De Casteljau: the control points of the cubic's first `t` of the curve
/// (its end point is the curve at `t`).
fn split_cubic_left(
    p0: [f64; 2],
    c1: [f64; 2],
    c2: [f64; 2],
    p1: [f64; 2],
    t: f64,
) -> ([f64; 2], [f64; 2], [f64; 2]) {
    let a = lerp(p0, c1, t);
    let b = lerp(c1, c2, t);
    let c = lerp(c2, p1, t);
    let ab = lerp(a, b, t);
    let bc = lerp(b, c, t);
    (a, ab, lerp(ab, bc, t))
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Not `f64::hypot`: that defers to the platform libm and may differ in the
/// last bit between native and wasm, while `sqrt` is IEEE-exact everywhere.
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

// --- parsing ---------------------------------------------------------------

struct Parser<'a> {
    text: &'a str,
    position: usize,
    segments: Vec<Segment>,
    current: [f64; 2],
    subpath_start: [f64; 2],
    /// Second control of the previous cubic, for `S` reflection.
    previous_cubic_control: Option<[f64; 2]>,
    /// Control of the previous quadratic, for `T` reflection.
    previous_quadratic_control: Option<[f64; 2]>,
    /// After `Z` the pen sits at the subpath start but a `MoveTo` has not
    /// been written; drawing resumes from there, so one is written first.
    subpath_closed: bool,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Parser {
            text,
            position: 0,
            segments: Vec::new(),
            current: [0.0, 0.0],
            subpath_start: [0.0, 0.0],
            previous_cubic_control: None,
            previous_quadratic_control: None,
            subpath_closed: false,
        }
    }

    fn run(&mut self) -> Result<(), PathDataError> {
        loop {
            self.skip_separators();
            if self.at_end() {
                return Ok(());
            }
            let command = self.read_command()?;
            self.apply(command)?;
            let repeated = implicit_repeat(command);
            while let Some(command) = repeated.filter(|_| self.number_follows()) {
                self.apply(command)?;
            }
        }
    }

    fn finish(self) -> Result<Vec<Segment>, PathDataError> {
        if self.segments.iter().all(|s| matches!(s, MoveTo(_))) {
            return Err(self.error_at(0, "path draws nothing"));
        }
        Ok(self.segments)
    }

    fn read_command(&mut self) -> Result<u8, PathDataError> {
        let offset = self.position;
        let byte = self.bytes()[offset];
        if !b"MmLlHhVvCcSsQqTtAaZz".contains(&byte) {
            let reason = if byte.is_ascii_alphabetic() {
                format!("unknown command '{}'", byte as char)
            } else {
                "expected a command letter".to_string()
            };
            return Err(self.error_at(offset, reason));
        }
        if self.segments.is_empty() && !matches!(byte, b'M' | b'm') {
            return Err(self.error_at(offset, "path data must start with a move command"));
        }
        self.position += 1;
        Ok(byte)
    }

    fn apply(&mut self, command: u8) -> Result<(), PathDataError> {
        let previous_cubic = self.previous_cubic_control.take();
        let previous_quadratic = self.previous_quadratic_control.take();
        let origin = if command.is_ascii_lowercase() { self.current } else { [0.0, 0.0] };

        match command.to_ascii_uppercase() {
            b'M' => {
                let point = self.read_point(origin)?;
                self.current = point;
                self.subpath_start = point;
                self.subpath_closed = false;
                self.segments.push(MoveTo(point));
            }
            b'L' => {
                let point = self.read_point(origin)?;
                self.line_to(point);
            }
            b'H' => {
                let x = origin[0] + self.read_number()?;
                self.line_to([x, self.current[1]]);
            }
            b'V' => {
                let y = origin[1] + self.read_number()?;
                self.line_to([self.current[0], y]);
            }
            b'C' => {
                let c1 = self.read_point(origin)?;
                let c2 = self.read_point(origin)?;
                let end = self.read_point(origin)?;
                self.cubic_to(c1, c2, end);
            }
            b'S' => {
                let c1 = previous_cubic.map_or(self.current, |c| reflect(c, self.current));
                let c2 = self.read_point(origin)?;
                let end = self.read_point(origin)?;
                self.cubic_to(c1, c2, end);
            }
            b'Q' => {
                let control = self.read_point(origin)?;
                let end = self.read_point(origin)?;
                self.quadratic_to(control, end);
            }
            b'T' => {
                let control = previous_quadratic.map_or(self.current, |c| reflect(c, self.current));
                let end = self.read_point(origin)?;
                self.quadratic_to(control, end);
            }
            b'A' => self.arc_command(origin)?,
            _ => self.close(),
        }
        Ok(())
    }

    fn arc_command(&mut self, origin: [f64; 2]) -> Result<(), PathDataError> {
        let radius_x = self.read_number()?;
        let radius_y = self.read_number()?;
        let rotation_degrees = self.read_number()?;
        let large_arc = self.read_flag()?;
        let sweep = self.read_flag()?;
        let end = self.read_point(origin)?;
        let arc = Arc { radius_x, radius_y, rotation_degrees, large_arc, sweep };
        self.begin_drawing();
        self.segments.extend(arc_to_segments(self.current, arc, end));
        self.current = end;
        Ok(())
    }

    fn line_to(&mut self, point: [f64; 2]) {
        self.begin_drawing();
        self.segments.push(LineTo(point));
        self.current = point;
    }

    fn cubic_to(&mut self, c1: [f64; 2], c2: [f64; 2], end: [f64; 2]) {
        self.begin_drawing();
        self.segments.push(CubicTo(c1, c2, end));
        self.previous_cubic_control = Some(c2);
        self.current = end;
    }

    /// Degree-elevates the quadratic to the cubic that traces it exactly.
    fn quadratic_to(&mut self, control: [f64; 2], end: [f64; 2]) {
        let start = self.current;
        let c1 = lerp(start, control, 2.0 / 3.0);
        let c2 = lerp(end, control, 2.0 / 3.0);
        self.cubic_to(c1, c2, end);
        self.previous_cubic_control = None;
        self.previous_quadratic_control = Some(control);
    }

    fn close(&mut self) {
        if !self.subpath_closed {
            self.segments.push(Close);
        }
        self.current = self.subpath_start;
        self.subpath_closed = true;
    }

    fn begin_drawing(&mut self) {
        if self.subpath_closed {
            self.segments.push(MoveTo(self.current));
            self.subpath_closed = false;
        }
    }

    // --- scanning ---

    fn bytes(&self) -> &'a [u8] {
        self.text.as_bytes()
    }

    fn at_end(&self) -> bool {
        self.position >= self.bytes().len()
    }

    fn skip_separators(&mut self) {
        while let Some(&byte) = self.bytes().get(self.position) {
            if !(byte.is_ascii_whitespace() || byte == b',') {
                break;
            }
            self.position += 1;
        }
    }

    fn number_follows(&mut self) -> bool {
        self.skip_separators();
        matches!(self.bytes().get(self.position), Some(b'0'..=b'9' | b'.' | b'+' | b'-'))
    }

    fn read_point(&mut self, origin: [f64; 2]) -> Result<[f64; 2], PathDataError> {
        let x = self.read_number()?;
        let y = self.read_number()?;
        Ok([origin[0] + x, origin[1] + y])
    }

    /// `sign? (digits ('.' digits?)? | '.' digits) (('e'|'E') sign? digits)?`
    fn read_number(&mut self) -> Result<f64, PathDataError> {
        self.skip_separators();
        let start = self.position;
        self.skip_sign();
        let integer_digits = self.skip_digits();
        let fraction_digits = if self.eat(b'.') { self.skip_digits() } else { 0 };
        if integer_digits + fraction_digits == 0 {
            return Err(self.error_at(start, "expected a number"));
        }
        if self.exponent_follows() {
            self.position += 1;
            self.skip_sign();
            self.skip_digits();
        }
        let number: f64 = self.text[start..self.position]
            .parse()
            .map_err(|_| self.error_at(start, "malformed number"))?;
        if !number.is_finite() {
            return Err(self.error_at(start, "number is out of range"));
        }
        Ok(number)
    }

    /// Arc flags are single characters, so `0010 10` is flag, flag, 10, 10.
    fn read_flag(&mut self) -> Result<bool, PathDataError> {
        self.skip_separators();
        let flag = match self.bytes().get(self.position) {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(self.error_at(self.position, "arc flag must be 0 or 1")),
        };
        self.position += 1;
        Ok(flag)
    }

    fn skip_sign(&mut self) {
        if matches!(self.bytes().get(self.position), Some(b'+' | b'-')) {
            self.position += 1;
        }
    }

    fn skip_digits(&mut self) -> usize {
        let start = self.position;
        while matches!(self.bytes().get(self.position), Some(b'0'..=b'9')) {
            self.position += 1;
        }
        self.position - start
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.bytes().get(self.position) == Some(&byte);
        if found {
            self.position += 1;
        }
        found
    }

    /// An `e` only starts an exponent when digits follow; otherwise it is
    /// left for the command scanner to reject.
    fn exponent_follows(&self) -> bool {
        let bytes = self.bytes();
        if !matches!(bytes.get(self.position), Some(b'e' | b'E')) {
            return false;
        }
        let after = match bytes.get(self.position + 1) {
            Some(b'+' | b'-') => self.position + 2,
            _ => self.position + 1,
        };
        matches!(bytes.get(after), Some(b'0'..=b'9'))
    }

    fn error_at(&self, offset: usize, reason: impl Into<String>) -> PathDataError {
        let last_byte = self.bytes().len().saturating_sub(1);
        PathDataError { offset: offset.min(last_byte), reason: reason.into() }
    }
}

/// What extra coordinate groups after a command mean: pairs after a move
/// are lines; `Z` takes none.
fn implicit_repeat(command: u8) -> Option<u8> {
    match command {
        b'M' => Some(b'L'),
        b'm' => Some(b'l'),
        b'Z' | b'z' => None,
        other => Some(other),
    }
}

fn reflect(control: [f64; 2], about: [f64; 2]) -> [f64; 2] {
    [2.0 * about[0] - control[0], 2.0 * about[1] - control[1]]
}

// --- arcs ------------------------------------------------------------------

struct Arc {
    radius_x: f64,
    radius_y: f64,
    rotation_degrees: f64,
    large_arc: bool,
    sweep: bool,
}

/// SVG 2 Appendix F.6.5-F.6.6: endpoint form to center form, then split
/// into cubics of at most a quarter turn each.
fn arc_to_segments(from: [f64; 2], arc: Arc, to: [f64; 2]) -> Vec<Segment> {
    if from == to {
        return Vec::new();
    }
    let mut rx = arc.radius_x.abs();
    let mut ry = arc.radius_y.abs();
    if rx == 0.0 || ry == 0.0 {
        return vec![LineTo(to)];
    }

    let phi = arc.rotation_degrees * PI / 180.0;
    let (sin_phi, cos_phi) = (libm::sin(phi), libm::cos(phi));

    let half_dx = (from[0] - to[0]) / 2.0;
    let half_dy = (from[1] - to[1]) / 2.0;
    let x1 = cos_phi * half_dx + sin_phi * half_dy;
    let y1 = -sin_phi * half_dx + cos_phi * half_dy;

    let too_small = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if too_small > 1.0 {
        let scale = too_small.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let numerator = rx2 * ry2 - rx2 * y1 * y1 - ry2 * x1 * x1;
    let denominator = rx2 * y1 * y1 + ry2 * x1 * x1;
    let sign = if arc.large_arc == arc.sweep { -1.0 } else { 1.0 };
    let coefficient = sign * (numerator / denominator).max(0.0).sqrt();
    let center_x1 = coefficient * rx * y1 / ry;
    let center_y1 = -coefficient * ry * x1 / rx;

    let center = [
        cos_phi * center_x1 - sin_phi * center_y1 + (from[0] + to[0]) / 2.0,
        sin_phi * center_x1 + cos_phi * center_y1 + (from[1] + to[1]) / 2.0,
    ];

    let start_vector = [(x1 - center_x1) / rx, (y1 - center_y1) / ry];
    let end_vector = [(-x1 - center_x1) / rx, (-y1 - center_y1) / ry];
    let start_angle = libm::atan2(start_vector[1], start_vector[0]);
    let mut sweep_angle = libm::atan2(
        start_vector[0] * end_vector[1] - start_vector[1] * end_vector[0],
        start_vector[0] * end_vector[0] + start_vector[1] * end_vector[1],
    );
    if !arc.sweep && sweep_angle > 0.0 {
        sweep_angle -= 2.0 * PI;
    } else if arc.sweep && sweep_angle < 0.0 {
        sweep_angle += 2.0 * PI;
    }

    let ellipse = Ellipse { center, rx, ry, cos_phi, sin_phi };
    let pieces = ((sweep_angle.abs() / FRAC_PI_2 - ARC_SPLIT_EPSILON).ceil() as usize).max(1);
    let step = sweep_angle / pieces as f64;
    let handle = 4.0 / 3.0 * libm::tan(step / 4.0);

    let mut segments = Vec::with_capacity(pieces);
    for piece in 0..pieces {
        let a = start_angle + step * piece as f64;
        let b = a + step;
        let (start, start_tangent) = ellipse.point_and_tangent(a);
        let (end, end_tangent) = ellipse.point_and_tangent(b);
        // The last endpoint is the one the document wrote, not a
        // reconstruction of it, so rounding cannot leave a gap.
        let end_point = if piece + 1 == pieces { to } else { end };
        segments.push(CubicTo(
            [start[0] + handle * start_tangent[0], start[1] + handle * start_tangent[1]],
            [end[0] - handle * end_tangent[0], end[1] - handle * end_tangent[1]],
            end_point,
        ));
    }
    segments
}

struct Ellipse {
    center: [f64; 2],
    rx: f64,
    ry: f64,
    cos_phi: f64,
    sin_phi: f64,
}

impl Ellipse {
    /// The point at angle `theta` and the derivative with respect to it.
    fn point_and_tangent(&self, theta: f64) -> ([f64; 2], [f64; 2]) {
        let (sin, cos) = (libm::sin(theta), libm::cos(theta));
        let rotate = |x: f64, y: f64| [self.cos_phi * x - self.sin_phi * y, self.sin_phi * x + self.cos_phi * y];
        let offset = rotate(self.rx * cos, self.ry * sin);
        let tangent = rotate(-self.rx * sin, self.ry * cos);
        ([self.center[0] + offset[0], self.center[1] + offset[1]], tangent)
    }
}
