use kineto_asciicast::{grid_states, parse_cast, CastError};

const FIXTURE: &str = include_str!("fixture.cast");

#[test]
fn parses_header_and_events() {
    let cast = parse_cast(FIXTURE).expect("fixture should parse");

    assert_eq!(cast.cols, 20);
    assert_eq!(cast.rows, 4);
    assert_eq!(cast.events.len(), 5);

    // Raw events are NOT coalesced by timestamp — that only happens in
    // grid_states. Two "o" events share t=0.5 (the chunked "typing").
    let times: Vec<f64> = cast.events.iter().map(|(t, _)| *t).collect();
    assert_eq!(times, vec![0.0, 0.5, 0.5, 1.2, 2.0]);
}

#[test]
fn bad_header_is_a_header_error() {
    let bad = "not a json header\n[0.0, \"o\", \"x\"]\n";

    match parse_cast(bad) {
        Err(CastError::Header(_)) => {}
        other => panic!("expected CastError::Header, got {other:?}"),
    }
}

#[test]
fn bad_event_line_is_an_event_error() {
    let bad = "{\"version\": 2, \"width\": 10, \"height\": 2}\nnot an event\n";

    match parse_cast(bad) {
        Err(CastError::Event { line, .. }) => assert_eq!(line, 2),
        other => panic!("expected CastError::Event, got {other:?}"),
    }
}

#[test]
fn grid_states_coalesces_same_timestamp_events_and_dedupes() {
    let cast = parse_cast(FIXTURE).expect("fixture should parse");
    let states = grid_states(&cast);

    // 5 raw events, but the two t=0.5 events coalesce into one grid ->
    // 4 distinct snapshots (0.0, 0.5, 1.2, 2.0), none of which repeat a
    // previous grid, so no further dedup collapsing happens.
    assert_eq!(states.len(), 4);

    let times: Vec<f64> = states.iter().map(|s| s.time_s).collect();
    assert_eq!(times, vec![0.0, 0.5, 1.2, 2.0]);
    for pair in times.windows(2) {
        assert!(pair[0] < pair[1], "times must be strictly increasing");
    }
}

#[test]
fn colored_ok_line_resolves_green_fg() {
    let cast = parse_cast(FIXTURE).expect("fixture should parse");
    let states = grid_states(&cast);

    // Index 2: after "$ kineto render\n" (index 0/1) comes the colored
    // "OK 42 frames" line.
    let state = &states[2];
    let row1 = &state.rows[1];

    assert_eq!(row1[0].ch, 'O');
    assert_eq!(row1[1].ch, 'K');
    assert_eq!(row1[0].fg, Some((0x4E, 0xBF, 0x22)));
    assert_eq!(row1[1].fg, Some((0x4E, 0xBF, 0x22)));

    // SGR reset before " 42 frames" clears the color back to default.
    assert_eq!(row1[2].ch, ' ');
    assert_eq!(row1[2].fg, None);
}

#[test]
fn clear_screen_resets_grid_except_new_prompt_line() {
    let cast = parse_cast(FIXTURE).expect("fixture should parse");
    let states = grid_states(&cast);

    let last = states.last().expect("at least one grid state");
    let row0: String = last.rows[0].iter().map(|c| c.ch).collect();

    assert_eq!(row0.trim_end(), "$");
    assert_eq!(&row0[..2], "$ ");

    for row in &last.rows[1..] {
        assert!(
            row.iter().all(|c| c.ch == ' '),
            "row should be blank after clear"
        );
    }
}
