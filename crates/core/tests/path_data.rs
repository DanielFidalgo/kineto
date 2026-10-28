use kineto_core::path_data::{from_points, parse, trim, PathDataError, Segment};
use Segment::{Close, CubicTo, LineTo, MoveTo};

fn assert_point_near(actual: [f64; 2], expected: [f64; 2], tolerance: f64) {
    assert!(
        (actual[0] - expected[0]).abs() <= tolerance
            && (actual[1] - expected[1]).abs() <= tolerance,
        "expected {expected:?}, got {actual:?} (tolerance {tolerance})"
    );
}

fn end_point(segment: &Segment) -> [f64; 2] {
    match *segment {
        MoveTo(p) | LineTo(p) | CubicTo(_, _, p) => p,
        Close => panic!("Close has no end point of its own"),
    }
}

fn cubic_at(p0: [f64; 2], c1: [f64; 2], c2: [f64; 2], p1: [f64; 2], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    let points = [p0, c1, c2, p1];
    let mut out = [0.0, 0.0];
    for (weight, point) in weights.iter().zip(points) {
        out[0] += weight * point[0];
        out[1] += weight * point[1];
    }
    out
}

fn parse_ok(d: &str) -> Vec<Segment> {
    parse(d).unwrap_or_else(|e| panic!("{d:?} should parse: {e}"))
}

fn expect_error(d: &str) -> PathDataError {
    parse(d).expect_err(&format!("{d:?} should be rejected"))
}

// --- parse -----------------------------------------------------------------

#[test]
fn absolute_commands_parse_to_segments() {
    assert_eq!(
        parse_ok("M0 0 L10 0 H20 V5 Z"),
        vec![
            MoveTo([0.0, 0.0]),
            LineTo([10.0, 0.0]),
            LineTo([20.0, 0.0]),
            LineTo([20.0, 5.0]),
            Close
        ]
    );
}

#[test]
fn relative_commands_are_resolved_against_the_current_point() {
    assert_eq!(
        parse_ok("m10 10 l5 0 h5 v5"),
        vec![
            MoveTo([10.0, 10.0]),
            LineTo([15.0, 10.0]),
            LineTo([20.0, 10.0]),
            LineTo([20.0, 15.0])
        ]
    );
}

#[test]
fn pairs_after_move_are_line_tos() {
    assert_eq!(
        parse_ok("M0 0 10 0 10 10"),
        vec![
            MoveTo([0.0, 0.0]),
            LineTo([10.0, 0.0]),
            LineTo([10.0, 10.0])
        ]
    );
}

#[test]
fn relative_move_after_close_starts_from_the_subpath_start() {
    let segments = parse_ok("M10 10 l10 0 z m5 5 l1 0");
    let second_move = segments.iter().filter(|s| matches!(s, MoveTo(_))).nth(1);
    assert_eq!(second_move, Some(&MoveTo([15.0, 15.0])));
}

#[test]
fn a_trailing_move_is_dropped() {
    let segments = parse_ok("M0 0 L10 10 M500 500");
    assert_eq!(segments.last(), Some(&LineTo([10.0, 10.0])));
}

#[test]
fn drawing_after_close_without_a_move_restarts_at_the_subpath_start() {
    assert_eq!(
        parse_ok("M10 10 l10 0 z l1 0"),
        vec![
            MoveTo([10.0, 10.0]),
            LineTo([20.0, 10.0]),
            Close,
            MoveTo([10.0, 10.0]),
            LineTo([11.0, 10.0])
        ]
    );
}

#[test]
fn quadratic_becomes_the_exact_cubic() {
    let segments = parse_ok("M0 0 Q 10 20 20 0");
    assert_eq!(segments.len(), 2);
    let CubicTo(c1, c2, end) = segments[1] else {
        panic!("expected a cubic, got {:?}", segments[1])
    };
    assert_point_near(c1, [20.0 / 3.0, 40.0 / 3.0], 1e-12);
    assert_point_near(c2, [40.0 / 3.0, 40.0 / 3.0], 1e-12);
    assert_point_near(end, [20.0, 0.0], 1e-12);
}

#[test]
fn smooth_quadratic_reflects_the_previous_quadratic_control() {
    // Reflection of (10, 20) about (20, 0) is (30, -20).
    let segments = parse_ok("M0 0 Q 10 20 20 0 T 40 0");
    let CubicTo(c1, _, end) = segments[2] else {
        panic!("expected a cubic, got {:?}", segments[2])
    };
    assert_point_near(c1, [20.0 + 2.0 / 3.0 * 10.0, 2.0 / 3.0 * -20.0], 1e-12);
    assert_point_near(end, [40.0, 0.0], 1e-12);
}

#[test]
fn smooth_cubic_reflects_the_previous_control() {
    let segments = parse_ok("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0");
    let CubicTo(c1, _, _) = segments[2] else {
        panic!("expected a cubic, got {:?}", segments[2])
    };
    assert_eq!(c1, [10.0, -10.0]);
}

#[test]
fn smooth_without_a_previous_curve_uses_the_current_point() {
    let segments = parse_ok("M0 0 S 10 10 20 0");
    let CubicTo(c1, _, _) = segments[1] else {
        panic!("expected a cubic, got {:?}", segments[1])
    };
    assert_eq!(c1, [0.0, 0.0]);
}

#[test]
fn number_grammar_edge_cases() {
    assert_eq!(
        parse_ok("M10-5L.5.5"),
        vec![MoveTo([10.0, -5.0]), LineTo([0.5, 0.5])]
    );
    assert_eq!(parse_ok("M1e1 0L0 0")[0], MoveTo([10.0, 0.0]));
}

#[test]
fn arc_flags_may_run_together() {
    let segments = parse_ok("M0 0 a10 10 0 0010 10");
    assert_point_near(end_point(segments.last().unwrap()), [10.0, 10.0], 1e-9);
}

#[test]
fn a_quarter_arc_is_one_cubic_on_the_circle() {
    let segments = parse_ok("M10 0 A10 10 0 0 1 0 10");
    let cubics: Vec<_> = segments
        .iter()
        .filter(|s| matches!(s, CubicTo(..)))
        .collect();
    assert_eq!(cubics.len(), 1);
    let CubicTo(c1, c2, end) = *cubics[0] else {
        unreachable!()
    };
    assert_point_near(end, [0.0, 10.0], 1e-9);
    let midpoint = cubic_at([10.0, 0.0], c1, c2, end, 0.5);
    let radius = midpoint[0].hypot(midpoint[1]);
    assert!((radius - 10.0).abs() < 0.01, "midpoint radius {radius}");
}

#[test]
fn a_half_arc_splits_into_quarter_segments() {
    let segments = parse_ok("M10 0 A10 10 0 0 1 -10 0");
    assert_eq!(
        segments.iter().filter(|s| matches!(s, CubicTo(..))).count(),
        2
    );
}

#[test]
fn a_zero_radius_arc_is_a_straight_line() {
    assert_eq!(
        parse_ok("M0 0 A0 5 0 0 1 10 10"),
        vec![MoveTo([0.0, 0.0]), LineTo([10.0, 10.0])]
    );
}

#[test]
fn an_arc_with_equal_endpoints_draws_nothing() {
    assert_eq!(
        parse_ok("M0 0 L5 5 A10 10 0 0 1 5 5"),
        vec![MoveTo([0.0, 0.0]), LineTo([5.0, 5.0])]
    );
}

#[test]
fn too_small_radii_are_scaled_up_to_reach_the_endpoint() {
    let segments = parse_ok("M0 0 A1 1 0 0 1 20 0");
    assert_point_near(end_point(segments.last().unwrap()), [20.0, 0.0], 1e-9);
    // Scaled radius is 10: every control point stays within a circle of that radius about (10, 0).
    for segment in &segments[1..] {
        let CubicTo(c1, c2, end) = *segment else {
            panic!("expected cubics, got {segment:?}")
        };
        for point in [c1, c2, end] {
            assert!(
                (point[0] - 10.0).hypot(point[1]) < 15.0,
                "{point:?} is far outside the scaled circle"
            );
        }
    }
}

#[test]
fn malformed_input_is_an_error_not_a_panic() {
    for d in ["L10 10", "M0 0 X", "M0", "M0 0 A10 10 0 2 0 5 5"] {
        let error = expect_error(d);
        assert!(
            error.offset < d.len(),
            "{d:?}: offset {} is outside the string",
            error.offset
        );
    }
}

#[test]
fn errors_point_at_the_offending_byte() {
    assert_eq!(expect_error("M0 0 X").offset, 5);
    assert_eq!(expect_error("M0 0 A10 10 0 2 0 5 5").offset, 14);
}

#[test]
fn a_number_where_a_command_is_expected_is_an_error() {
    assert!(parse("M0 0 Z 5 5").is_err());
}

#[test]
fn a_number_too_large_for_a_float_is_an_error() {
    assert!(parse("M1e999 0 L1 1").is_err());
}

#[test]
fn a_string_with_no_drawing_command_is_an_error() {
    for d in ["", "M5 5"] {
        let error = expect_error(d);
        assert!(
            error.reason.contains("draws nothing"),
            "{d:?}: {}",
            error.reason
        );
    }
}

#[test]
fn from_points_emits_move_lines_and_an_optional_close() {
    let points = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
    assert_eq!(
        from_points(&points, false),
        vec![
            MoveTo([0.0, 0.0]),
            LineTo([10.0, 0.0]),
            LineTo([10.0, 10.0])
        ]
    );
    assert_eq!(from_points(&points, true).last(), Some(&Close));
    assert!(from_points(&[], true).is_empty());
}

// --- trim ------------------------------------------------------------------

fn two_contour_path() -> Vec<Segment> {
    parse_ok("M0 0 C 0 10 10 10 10 0 L 10 -10 Z M20 0 L30 0 L30 10 Z")
}

fn closed_square() -> Vec<Segment> {
    from_points(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], true)
}

#[test]
fn trim_at_one_returns_the_input_unchanged() {
    let path = two_contour_path();
    assert_eq!(trim(&path, 1.0), path);
}

#[test]
fn trim_at_zero_or_below_is_empty() {
    let path = two_contour_path();
    assert!(trim(&path, 0.0).is_empty());
    assert!(trim(&path, -0.2).is_empty());
}

#[test]
fn trim_of_nan_is_empty() {
    assert!(trim(&two_contour_path(), f64::NAN).is_empty());
}

#[test]
fn trim_halves_a_straight_line() {
    let line = from_points(&[[0.0, 0.0], [10.0, 0.0]], false);
    assert_eq!(
        trim(&line, 0.5),
        vec![MoveTo([0.0, 0.0]), LineTo([5.0, 0.0])]
    );
}

#[test]
fn subpaths_draw_in_order_as_one_pen() {
    let path = parse_ok("M0 0 L10 0 M0 5 L10 5");
    assert_eq!(
        trim(&path, 0.75),
        vec![
            MoveTo([0.0, 0.0]),
            LineTo([10.0, 0.0]),
            MoveTo([0.0, 5.0]),
            LineTo([5.0, 5.0])
        ]
    );
}

#[test]
fn a_subpath_is_not_started_once_the_pen_has_run_out() {
    let path = parse_ok("M0 0 L10 0 M0 5 L10 5");
    assert_eq!(
        trim(&path, 0.5),
        vec![MoveTo([0.0, 0.0]), LineTo([10.0, 0.0])]
    );
}

#[test]
fn a_partial_closed_contour_has_no_closing_edge() {
    assert_eq!(
        trim(&closed_square(), 0.5),
        vec![
            MoveTo([0.0, 0.0]),
            LineTo([10.0, 0.0]),
            LineTo([10.0, 10.0])
        ]
    );
    assert_eq!(trim(&closed_square(), 1.0).last(), Some(&Close));
}

#[test]
fn a_partly_drawn_closing_edge_becomes_a_line() {
    // Perimeter 40, so 0.875 stops halfway down the closing edge from (0,10) to (0,0).
    let trimmed = trim(&closed_square(), 0.875);
    assert_eq!(trimmed.len(), 5);
    assert_point_near(end_point(trimmed.last().unwrap()), [0.0, 5.0], 1e-9);
    assert!(!trimmed.contains(&Close));
}

#[test]
fn a_trimmed_cubic_stays_on_the_original_curve() {
    let (p0, c1, c2, p1) = ([0.0, 0.0], [0.0, 30.0], [40.0, 30.0], [40.0, 0.0]);
    let curve = vec![MoveTo(p0), CubicTo(c1, c2, p1)];
    let trimmed = trim(&curve, 0.5);
    assert_eq!(trimmed.len(), 2);
    let CubicTo(_, _, end) = trimmed[1] else {
        panic!("expected a cubic, got {:?}", trimmed[1])
    };

    // Dense-sample the original: the endpoint must lie on it, and half its length must lie behind it.
    let samples = 20_000;
    let mut nearest = (f64::MAX, 0.0);
    let mut walked = 0.0;
    let mut previous = p0;
    let mut length_to_nearest = 0.0;
    for i in 1..=samples {
        let t = i as f64 / samples as f64;
        let point = cubic_at(p0, c1, c2, p1, t);
        walked += (point[0] - previous[0]).hypot(point[1] - previous[1]);
        previous = point;
        let distance = (point[0] - end[0]).hypot(point[1] - end[1]);
        if distance < nearest.0 {
            nearest = (distance, t);
            length_to_nearest = walked;
        }
    }
    assert!(nearest.0 < 0.01, "endpoint is {} off the curve", nearest.0);
    assert!(
        (length_to_nearest / walked - 0.5).abs() < 0.01,
        "split at {} of the curve length",
        length_to_nearest / walked
    );
}

#[test]
fn a_trimmed_cubic_keeps_its_start_and_tangent() {
    let curve = vec![
        MoveTo([0.0, 0.0]),
        CubicTo([0.0, 30.0], [40.0, 30.0], [40.0, 0.0]),
    ];
    let trimmed = trim(&curve, 0.25);
    assert_eq!(trimmed[0], MoveTo([0.0, 0.0]));
    let CubicTo(c1, _, _) = trimmed[1] else {
        panic!("expected a cubic")
    };
    assert_eq!(
        c1[0], 0.0,
        "left half of a curve that leaves vertically still leaves vertically"
    );
}
