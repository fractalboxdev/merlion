//! Sequence layout (specs/sequence.md#layout): columns, rows, activations, notes,
//! fragments, boxes, create and destroy, container fit and fuel. Models are built by
//! hand, so nothing here depends on the sequence parser.

#[path = "layout_sequence_support.rs"]
mod support;

use merlion_render::geometry::sequence::{ACTIVATION_NEST, ACTIVATION_W, COLUMN_GAP_MIN};
use merlion_render::layout::sequence::{
    ACTOR_FIGURE, BOX_PAD, FRAGMENT_PAD, HEAD_MIN, HEAD_PAD, ROW_GAP, SELF_HEIGHT, SELF_WIDTH,
};
use merlion_render::layout::{LayoutError, MARGIN};
use merlion_render::model::sequence::{Central, FragmentKind, Placement};
use merlion_render::options::RenderOptions;
use support::*;

fn two_messages() -> merlion_render::model::sequence::Sequence {
    let mut s = S::new();
    let v = s.ps(&["Alice", "John"]);
    let a = s.msg(v[0], v[1], "Hello John, how are you?");
    let b = s.msg(v[1], v[0], "Great!");
    s.done(vec![a, b])
}

// ---------------------------------------------------------------------------
// Columns

#[test]
fn columns_follow_source_order_and_clear_each_other() {
    let seq = two_messages();
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.participants.len(), 2);
    let (a, b) = (&g.participants[0], &g.participants[1]);
    // The gap between the head boxes is `node_spacing` when nothing needs more.
    let gap = (b.head.x) - (a.head.x + a.head.w);
    assert!(gap >= COLUMN_GAP_MIN - EPS, "gap {gap}");
    assert!(a.head.x >= MARGIN - EPS, "left margin {}", a.head.x);
}

#[test]
fn a_head_box_is_the_label_plus_padding_and_never_below_the_minimum() {
    let mut s = S::new();
    s.p("A");
    s.labelled("B", "A considerably longer participant label");
    let seq = s.done(vec![]);
    let g = run(&seq);
    check(&seq, &g);
    let a = &g.participants[0];
    assert!(
        (a.head.w - HEAD_MIN.0).abs() < EPS,
        "a short label keeps the minimum width: {}",
        a.head.w
    );
    assert!(a.head.h >= HEAD_MIN.1 - EPS);
    let b = &g.participants[1];
    assert!(
        (b.head.w - (b.label.width + 2.0 * HEAD_PAD.0)).abs() < EPS,
        "head {} label {}",
        b.head.w,
        b.label.width
    );
}

#[test]
fn an_actor_head_carries_the_stick_figure_above_its_label() {
    let mut s = S::new();
    s.p("A");
    s.actor("B");
    let seq = s.done(vec![]);
    let g = run(&seq);
    check(&seq, &g);
    let (plain, actor) = (&g.participants[0], &g.participants[1]);
    assert!(
        actor.head.h >= plain.head.h + ACTOR_FIGURE.1,
        "actor {} plain {}",
        actor.head.h,
        plain.head.h
    );
}

#[test]
fn a_wide_message_label_widens_the_gap_it_spans() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let m = s.msg(
        v[0],
        v[1],
        "A message label wide enough to push the columns apart",
    );
    let seq = s.done(vec![m]);
    let g = run_with(
        &seq,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .expect("layout");
    check(&seq, &g);
    let label = g.messages[0].label.as_ref().expect("label").2.width;
    let span = g.participants[1].x - g.participants[0].x;
    assert!(span >= label, "span {span} label {label}");
}

#[test]
fn a_self_message_reserves_room_right_of_its_own_column() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let m = s.msg(v[0], v[0], "checks its own state");
    let seq = s.done(vec![m]);
    let g = run_with(
        &seq,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .expect("layout");
    check(&seq, &g);
    let m = &g.messages[0];
    assert!(m.self_loop);
    let label = m.label.as_ref().expect("label");
    let right = label.0 + label.2.width / 2.0;
    assert!(
        right <= g.participants[1].head.x + EPS,
        "the bracket and its label run into column B: {right} vs {}",
        g.participants[1].head.x
    );
    assert!(
        label.0 - label.2.width / 2.0 >= g.participants[0].x + SELF_WIDTH - EPS,
        "the label crowds the bracket: {} vs {}",
        label.0 - label.2.width / 2.0,
        g.participants[0].x + SELF_WIDTH
    );
    assert!(
        m.to.1 - m.from.1 >= SELF_HEIGHT - EPS,
        "self bracket height {}",
        m.to.1 - m.from.1
    );
}

#[test]
fn a_participant_box_pads_its_run_away_from_the_neighbours() {
    let mut plain = S::new();
    plain.ps(&["A", "B", "C"]);
    let plain = plain.done(vec![]);
    let bare = run(&plain);

    let mut s = S::new();
    let v = s.ps(&["A", "B", "C"]);
    s.group("Front", &[v[0], v[1]]);
    let seq = s.done(vec![]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.boxes.len(), 1);
    let b = &g.boxes[0];
    assert!(
        contains(&b.box_, &g.participants[0].head) && contains(&b.box_, &g.participants[1].head),
        "the box misses its members: {:?}",
        b.box_
    );
    assert!(
        !overlaps(&b.box_, &g.participants[2].head),
        "the box swallows the neighbour"
    );
    assert!(
        b.box_.x <= g.participants[0].head.x - BOX_PAD + EPS,
        "box pad on the left"
    );
    assert!(g.width > bare.width, "the box widens the drawing");
}

// ---------------------------------------------------------------------------
// Rows

#[test]
fn rows_advance_down_the_page_in_item_order() {
    let seq = two_messages();
    let g = run(&seq);
    check(&seq, &g);
    assert!(
        g.messages[1].to.1 > g.messages[0].to.1 + EPS,
        "rows out of order"
    );
    let head_bottom = g
        .participants
        .iter()
        .map(|p| p.head.y + p.head.h)
        .fold(0.0f64, f64::max);
    assert!(
        g.messages[0].to.1 >= head_bottom + ROW_GAP - EPS,
        "the first row starts under the heads"
    );
}

#[test]
fn a_message_label_sits_above_its_arrow_and_between_its_columns() {
    let seq = two_messages();
    let g = run(&seq);
    check(&seq, &g);
    let m = &g.messages[0];
    let (x, y, l) = m.label.as_ref().expect("label");
    assert!(*y + l.height / 2.0 <= m.to.1 + EPS, "label below the arrow");
    let mid = (m.from.0 + m.to.0) / 2.0;
    assert!((x - mid).abs() < EPS, "label {x} not centred on {mid}");
}

#[test]
fn an_empty_label_draws_nothing_and_still_takes_a_row() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let m = s.msg(v[0], v[1], "");
    let seq = s.done(vec![m]);
    let g = run(&seq);
    check(&seq, &g);
    assert!(g.messages[0].label.is_none());
    let head_bottom = g.participants[0].head.y + g.participants[0].head.h;
    assert!(g.messages[0].to.1 > head_bottom + ROW_GAP - EPS);
}

#[test]
fn a_note_takes_its_own_row_and_clears_the_rows_around_it() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let m1 = s.msg(v[0], v[1], "first");
    let n = s.note(Placement::Over, v[0], v[1], "One order, one transaction");
    let m2 = s.msg(v[1], v[0], "second");
    let seq = s.done(vec![m1, n, m2]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.notes.len(), 1);
    let note = &g.notes[0];
    assert!(note.box_.y >= g.messages[0].to.1 - EPS, "note above row 0");
    assert!(
        note.box_.y + note.box_.h <= g.messages[1].to.1 + EPS,
        "note below row 1"
    );
    assert_eq!(note.placement, Placement::Over);
}

#[test]
fn a_note_beside_a_participant_clears_its_neighbour() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let n = s.note(Placement::RightOf, v[0], v[0], "A long aside about A");
    let seq = s.done(vec![n]);
    let g = run_with(
        &seq,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .expect("layout");
    check(&seq, &g);
    let note = &g.notes[0];
    assert!(note.box_.x >= g.participants[0].x - EPS, "note left of A");
    assert!(
        note.box_.x + note.box_.w <= g.participants[1].head.x + EPS,
        "note runs into B"
    );
}

#[test]
fn a_note_left_of_the_first_column_stays_on_the_page() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let n = s.note(Placement::LeftOf, v[0], v[0], "before anything happens");
    let seq = s.done(vec![n]);
    let g = run(&seq);
    check(&seq, &g);
    assert!(g.notes[0].box_.x >= MARGIN - EPS);
}

// ---------------------------------------------------------------------------
// Activations, create and destroy

#[test]
fn an_activation_bar_runs_from_its_opening_row_to_its_closing_row() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let open = s.msg_act(v[0], v[1], "call", true, false);
    let close = s.msg_act(v[1], v[0], "return", false, true);
    let seq = s.done(vec![open, close]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.activations.len(), 1);
    let bar = &g.activations[0];
    assert_eq!(bar.participant, 1);
    assert_eq!(bar.depth, 0);
    assert!((bar.bar.w - ACTIVATION_W).abs() < EPS);
    assert!(bar.bar.y <= g.messages[0].to.1 + EPS, "bar opens late");
    assert!(
        bar.bar.y + bar.bar.h >= g.messages[1].from.1 - EPS,
        "bar closes early"
    );
    // Both ends of the pair land on the bar, not on B's lifeline.
    assert!((g.messages[0].to.0 - bar.bar.x).abs() < EPS, "arrival edge");
    assert!(
        (g.messages[1].from.0 - bar.bar.x).abs() < EPS,
        "departure edge"
    );
}

#[test]
fn nested_activations_step_right_by_the_nesting_offset() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let outer = s.activate(v[1]);
    let inner = s.activate(v[1]);
    let m = s.msg(v[0], v[1], "into the inner bar");
    let close_inner = s.deactivate(v[1]);
    let close_outer = s.deactivate(v[1]);
    let seq = s.done(vec![outer, inner, m, close_inner, close_outer]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.activations.len(), 2);
    let (a, b) = (&g.activations[0], &g.activations[1]);
    assert_eq!((a.depth, b.depth), (0, 1));
    assert!(
        (b.bar.x - a.bar.x - ACTIVATION_NEST).abs() < EPS,
        "nesting offset: {} {}",
        a.bar.x,
        b.bar.x
    );
    assert!(
        (g.messages[0].to.0 - b.bar.x).abs() < EPS,
        "the arrow lands on the innermost bar"
    );
}

#[test]
fn an_activation_left_open_closes_at_the_last_row() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let open = s.msg_act(v[0], v[1], "call", true, false);
    let last = s.msg(v[1], v[0], "and nothing closes it");
    let seq = s.done(vec![open, last]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.activations.len(), 1);
    let bar = &g.activations[0];
    assert!(
        bar.bar.y + bar.bar.h >= g.messages[1].to.1 - EPS,
        "the open bar stops above the last row"
    );
}

#[test]
fn a_deactivate_with_nothing_open_is_dropped() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let stray = s.deactivate(v[1]);
    let m = s.msg(v[0], v[1], "hi");
    let seq = s.done(vec![stray, m]);
    let g = run(&seq);
    check(&seq, &g);
    assert!(g.activations.is_empty());
}

#[test]
fn a_created_participant_starts_at_its_creating_row() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let first = s.msg(v[0], v[0], "warm up");
    let create = s.peek();
    let m = s.msg(v[0], v[1], "create");
    s.seq.participants[v[1]].created_by = Some(create);
    let seq = s.done(vec![first, m]);
    let g = run(&seq);
    check(&seq, &g);
    let b = &g.participants[1];
    assert!(
        b.head.y > g.participants[0].head.y + EPS,
        "the created head stays at the top: {:?}",
        b.head
    );
    assert!(
        (b.head.y + b.head.h / 2.0 - g.messages[create as usize].to.1).abs() < HEAD_MIN.1,
        "the created head is not on its row"
    );
}

#[test]
fn a_destroyed_participant_loses_its_foot_box() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let kill = s.peek();
    let m = s.msg(v[0], v[1], "destroy");
    let after = s.msg(v[0], v[0], "carries on");
    s.seq.participants[v[1]].destroyed_by = Some(kill);
    let seq = s.done(vec![m, after]);
    let g = run(&seq);
    check(&seq, &g);
    assert!(
        g.participants[1].foot.is_none(),
        "destroyed heads keep a foot"
    );
    assert!(g.participants[0].foot.is_some());
    assert!(
        (g.participants[1].lifeline.1 - g.messages[kill as usize].to.1).abs() < EPS,
        "the lifeline outlives the destroying row"
    );
}

// ---------------------------------------------------------------------------
// Fragments

#[test]
fn a_fragment_encloses_its_rows() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let inner1 = s.msg(v[0], v[1], "poll");
    let inner2 = s.msg(v[1], v[0], "pong");
    let f = frag(
        FragmentKind::Loop,
        vec![section("Every minute", vec![inner1, inner2])],
    );
    let after = s.msg(v[0], v[1], "done");
    let seq = s.done(vec![f, after]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.fragments.len(), 1);
    let fg = &g.fragments[0];
    assert_eq!(fg.kind, FragmentKind::Loop);
    assert_eq!(fg.depth, 0);
    assert!(fg.sections.is_empty(), "one section, no dividers");
    assert!(
        fg.box_.y + fg.box_.h <= g.messages[2].to.1 + EPS,
        "the box swallows the row after it"
    );
    assert!(
        fg.box_.x <= g.participants[0].x - FRAGMENT_PAD + EPS,
        "the box hugs the left column too closely"
    );
}

#[test]
fn a_two_section_fragment_carries_one_divider() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let a = s.msg(v[0], v[1], "sick");
    let b = s.msg(v[1], v[0], "well");
    let f = frag(
        FragmentKind::Alt,
        vec![section("is sick", vec![a]), section("is well", vec![b])],
    );
    let seq = s.done(vec![f]);
    let g = run(&seq);
    check(&seq, &g);
    let fg = &g.fragments[0];
    assert_eq!(fg.sections.len(), 1);
    let d = &fg.sections[0];
    assert!(
        d.y > g.messages[0].to.1 && d.y < g.messages[1].to.1,
        "the divider is not between the sections"
    );
}

#[test]
fn nested_fragments_inset_inside_their_parent() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let deep = s.msg(v[0], v[1], "deep");
    let inner = frag(FragmentKind::Opt, vec![section("maybe", vec![deep])]);
    let outer = frag(FragmentKind::Loop, vec![section("always", vec![inner])]);
    let seq = s.done(vec![outer]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.fragments.len(), 2);
    let (o, i) = (&g.fragments[0], &g.fragments[1]);
    assert_eq!((o.depth, i.depth), (0, 1));
    assert!(o.box_.x <= i.box_.x - FRAGMENT_PAD + EPS, "left inset");
    assert!(
        o.box_.x + o.box_.w >= i.box_.x + i.box_.w + FRAGMENT_PAD - EPS,
        "right inset"
    );
}

#[test]
fn an_empty_fragment_still_draws_a_box() {
    let mut s = S::new();
    s.ps(&["A", "B"]);
    let f = frag(FragmentKind::Break, vec![section("nothing", vec![])]);
    let seq = s.done(vec![f]);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.fragments.len(), 1);
    assert!(g.fragments[0].box_.h > 0.0);
}

// ---------------------------------------------------------------------------
// Autonumber

#[test]
fn autonumber_places_a_badge_on_every_message() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    s.autonumber();
    let a = s.msg(v[0], v[1], "one");
    let b = s.msg(v[1], v[0], "two");
    let seq = s.done(vec![a, b]);
    let g = run(&seq);
    check(&seq, &g);
    assert!(g.messages.iter().all(|m| m.number.is_some()));
    let plain = {
        let mut t = S::new();
        let v = t.ps(&["A", "B"]);
        let a = t.msg(v[0], v[1], "one");
        let b = t.msg(v[1], v[0], "two");
        t.done(vec![a, b])
    };
    assert!(run(&plain).messages.iter().all(|m| m.number.is_none()));
}

// ---------------------------------------------------------------------------
// Container fit and determinism

#[test]
fn a_wide_diagram_narrows_toward_the_target() {
    let mut s = S::new();
    let v = [
        s.labelled("a", "Customer relationship manager"),
        s.labelled("b", "Payment authorisation service"),
        s.labelled("c", "Settlement ledger and journal"),
        s.labelled("d", "Notification delivery gateway"),
    ];
    let m = s.msg(v[0], v[3], "kick off");
    let seq = s.done(vec![m]);
    let loose = run_with(
        &seq,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .expect("layout");
    assert!(loose.width > 720.0, "precondition: {}", loose.width);
    let g = run(&seq);
    check(&seq, &g);
    assert!(g.width < loose.width, "fit did nothing: {}", g.width);
    assert!(g.width <= 720.0 + EPS, "width {}", g.width);
}

#[test]
fn a_diagram_that_cannot_fit_keeps_its_labels_unwrapped() {
    let mut s = S::new();
    let v = [
        s.labelled("a", "Customer relationship manager"),
        s.labelled("b", "Payment authorisation service"),
        s.labelled("c", "Settlement ledger and journal"),
        s.labelled("d", "Notification delivery gateway"),
        s.labelled("e", "Reconciliation batch runner"),
    ];
    let m = s.msg(v[0], v[1], "declined (insufficient funds)");
    let seq = s.done(vec![m]);
    let g = run(&seq);
    check(&seq, &g);
    // The head boxes alone overflow the target, so no wrap width reaches it.
    assert!(
        g.width > 720.0,
        "precondition: the diagram fits after all: {}",
        g.width
    );
    let (_, _, label) = g.messages[0].label.as_ref().expect("message label");
    assert_eq!(
        label.lines.len(),
        1,
        "the label wrapped for a fit the diagram never reaches"
    );
}

#[test]
fn fit_never_shrinks_a_gap_past_the_minimum() {
    let mut s = S::new();
    let v = s.ps(&["A", "B", "C", "D", "E", "F", "G", "H"]);
    let m = s.msg(v[0], v[7], "across the diagram");
    let seq = s.done(vec![m]);
    let g = run_with(
        &seq,
        &RenderOptions {
            target_width: 1.0,
            ..RenderOptions::default()
        },
    )
    .expect("layout");
    check(&seq, &g);
    for w in g.participants.windows(2) {
        let gap = w[1].head.x - (w[0].head.x + w[0].head.w);
        assert!(gap >= COLUMN_GAP_MIN - EPS, "gap {gap} below the minimum");
    }
}

#[test]
fn the_same_model_lays_out_identically_every_time() {
    let seq = {
        let mut s = S::new();
        let v = s.ps(&["A", "B", "C"]);
        s.group("Front", &[v[0], v[1]]);
        s.autonumber();
        let m1 = s.msg_act(v[0], v[1], "call", true, false);
        let n = s.note(Placement::Over, v[1], v[2], "a note across two columns");
        let inner = s.msg(v[1], v[2], "delegate");
        let m2 = s.msg_act(v[1], v[0], "return", false, true);
        let self_m = s.msg(v[2], v[2], "retry");
        let f = frag(
            FragmentKind::Alt,
            vec![
                section("first", vec![inner]),
                section("second", vec![self_m]),
            ],
        );
        s.done(vec![m1, n, f, m2])
    };
    let a = run(&seq);
    let b = run(&seq);
    assert_eq!(a, b);
    check(&seq, &a);
}

// ---------------------------------------------------------------------------
// Limits and fuel

#[test]
fn a_large_diagram_lays_out_within_the_default_fuel() {
    let mut s = S::new();
    let ids: Vec<String> = (0..60).map(|i| format!("p{i}")).collect();
    let v: Vec<usize> = ids.iter().map(|i| s.p(i)).collect();
    let mut items = Vec::new();
    for i in 0..600usize {
        let from = v[i % v.len()];
        let to = v[(i * 7 + 3) % v.len()];
        items.push(s.msg(from, to, "step"));
    }
    let seq = s.done(items);
    let g = run(&seq);
    check(&seq, &g);
    assert_eq!(g.messages.len(), 600);
    assert!(g.fuel_used > 0);
}

#[test]
fn exhausted_fuel_is_too_large_not_a_panic() {
    let seq = two_messages();
    assert_eq!(
        run_fuel(&seq, 1),
        Err(LayoutError::TooLarge { what: "fuel" })
    );
}

#[test]
fn a_model_with_no_participants_still_produces_a_page() {
    let seq = S::new().done(vec![]);
    let g = run(&seq);
    assert!(g.width > 0.0 && g.height > 0.0);
    assert!(g.participants.is_empty());
}

#[test]
fn out_of_range_participant_indices_are_dropped_not_indexed() {
    let mut s = S::new();
    let v = s.ps(&["A"]);
    let bad = s.msg(v[0], 9, "nowhere");
    let note = s.note(Placement::Over, 7, 9, "adrift");
    let seq = s.done(vec![bad, note]);
    let g = run(&seq);
    check_finite(&g);
    assert_eq!(g.participants.len(), 1);
}

#[test]
fn hostile_render_options_produce_finite_geometry() {
    let seq = two_messages();
    let g = run_with(
        &seq,
        &RenderOptions {
            target_width: f64::NAN,
            node_spacing: -1e9,
            font_size: f64::INFINITY,
            wrap_width: 0.0,
            ..RenderOptions::default()
        },
    )
    .expect("layout");
    check_finite(&g);
    check_columns(&seq, &g);
}

#[test]
fn a_central_connection_lands_on_the_lifeline_not_the_activation_bar() {
    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let open = s.activate(v[1]);
    let plain = s.msg(v[0], v[1], "on the bar");
    let central = s.msg_opt(v[0], v[1], "on the lifeline", Central::Target, None);
    let seq = s.done(vec![open, plain, central]);
    let g = run(&seq);
    check(&seq, &g);
    let bar = &g.activations[0];
    assert!(
        (g.messages[0].to.0 - bar.bar.x).abs() < EPS,
        "a plain arrow lands on the bar"
    );
    assert!(
        (g.messages[1].to.0 - g.participants[1].x).abs() < EPS,
        "a central connection lands on the lifeline: {} vs {}",
        g.messages[1].to.0,
        g.participants[1].x
    );
}

#[test]
fn nowrap_keeps_a_label_on_one_line_and_widens_the_gap() {
    let long = "a nowrap label that is far wider than the wrap width allows for one line";
    let mut wrapped = S::new();
    let v = wrapped.ps(&["A", "B"]);
    let m = wrapped.msg(v[0], v[1], long);
    let wrapped = wrapped.done(vec![m]);

    let mut s = S::new();
    let v = s.ps(&["A", "B"]);
    let m = s.msg_opt(v[0], v[1], long, Central::None, Some(false));
    let seq = s.done(vec![m]);

    let loose = RenderOptions {
        target_width: 1e9,
        ..RenderOptions::default()
    };
    let a = run_with(&wrapped, &loose).expect("layout");
    let b = run_with(&seq, &loose).expect("layout");
    check(&seq, &b);
    let one_line = b.messages[0].label.as_ref().expect("label");
    assert_eq!(one_line.2.lines.len(), 1, "nowrap wrapped the label");
    assert!(
        b.width > a.width,
        "nowrap {} is no wider than the wrapped {}",
        b.width,
        a.width
    );
}
