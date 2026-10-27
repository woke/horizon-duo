//! Two fingers on an inventory: finger 1 rests on the stack in slot 3 at `A`, and the others
//! deal from it.

use super::*;

const C: [f64; 2] = [1400.0, 800.0];
const D: [f64; 2] = [1500.0, 800.0];

/// The stack finger 1 rests on.
const SOURCE: SlotPoint = SlotPoint { slot: 3, at: A };

const fn begin(deal: u64) -> Input {
    Input::Deal(Deal::Begin {
        deal,
        source: SOURCE,
    })
}

const fn end(deal: u64) -> Input {
    Input::Deal(Deal::End { deal })
}

fn one(stroke: u64, slot: u32, at: [f64; 2]) -> Input {
    Input::Deal(Deal::Commit {
        deal: 1,
        stroke,
        share: Share::One(SlotPoint { slot, at }),
    })
}

fn even(stroke: u64, targets: &[(u32, [f64; 2])]) -> Input {
    Input::Deal(Deal::Commit {
        deal: 1,
        stroke,
        share: Share::Even(
            targets
                .iter()
                .map(|&(slot, at)| SlotPoint { slot, at })
                .collect(),
        ),
    })
}

/// The game's answer to every deal in `out`.
fn answer(touch: &mut Touch, out: &[Input], answer: Dealt, now: Instant) {
    for input in out {
        if let Input::Deal(deal) = input {
            touch.dealt(deal, answer, now);
        }
    }
}

/// An inventory where finger 1 rests on slot 3's stack and finger 2 has landed at `B` over
/// `here`, the deal begun.
fn dealing(here: Here) -> (Touch, Layout, Instant) {
    let (mut touch, layout, now) = inventory();
    assert_eq!(
        touch.on(&layout, event(1, Phase::Start, A, slot(3)), now),
        [Input::Pointer(A)]
    );
    let out = touch.on(&layout, event(2, Phase::Start, B, here), now);
    assert_eq!(out, [begin(1)]);
    answer(&mut touch, &out, Dealt::Done, now);
    (touch, layout, now)
}

/// Whether a mark is drawn on the square above `at`, where a refusal's cross shows.
fn marked_above(touch: &Touch, layout: &Layout, at: [f64; 2], now: Instant) -> bool {
    let atlas =
        crate::hud::HudAtlas::build(std::path::Path::new("/nonexistent/horizon-test-assets"))
            .unwrap();
    let mut batch = crate::hud::HudBatch::new([2622, 1206]);
    touch.draw(&mut batch, &atlas, layout, false, now);
    let [x, y, w, h] = layout
        .above([at[0] as f32, at[1] as f32])
        .map(|v| v / layout.gui);
    batch
        .vertices
        .iter()
        .any(|v| (x..x + w).contains(&v.pos[0]) && (y..y + h).contains(&v.pos[1]))
}

#[test]
fn a_second_finger_beside_a_resting_stack_begins_a_deal_and_clicks_nothing() {
    let (mut touch, layout, now) = dealing(empty(4));
    assert!(tick(&mut touch, ms(now, 5000)).is_empty());
    assert!(
        touch
            .deal_view(&layout)
            .is_some_and(|view| view.loupe.is_some())
    );
}

#[test]
fn a_tap_deals_one_item_onto_the_slot_tapped() {
    let (mut touch, layout, now) = dealing(empty(4));
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        [one(1, 4, B)]
    );
}

#[test]
fn each_tap_is_a_stroke_of_its_own_however_quick() {
    let (mut touch, layout, now) = dealing(empty(4));
    touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
    for (id, stroke) in [(3, 2), (4, 3)] {
        assert!(
            touch
                .on(&layout, event(id, Phase::Start, B, empty(4)), now)
                .is_empty()
        );
        assert_eq!(
            touch.on(&layout, event(id, Phase::End, B, empty(4)), now),
            [one(stroke, 4, B)],
            "no double click, only one more item"
        );
    }
}

#[test]
fn a_dealing_finger_held_still_repeats_nothing() {
    let (mut touch, layout, now) = dealing(empty(4));
    for millis in (0..=5000).step_by(100) {
        assert!(tick(&mut touch, ms(now, millis)).is_empty(), "{millis} ms");
    }
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), ms(now, 5000)),
        [one(1, 4, B)]
    );
}

#[test]
fn a_drag_shares_the_stack_over_the_slots_it_reaches_in_order_each_once() {
    let (mut touch, layout, now) = dealing(empty(4));
    for (at, over) in [(C, empty(5)), (D, empty(6)), (B, empty(4))] {
        assert!(
            touch
                .on(&layout, event(2, Phase::Move, at, over), now)
                .is_empty()
        );
    }
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        [even(1, &[(4, B), (5, C), (6, D)])]
    );
}

#[test]
fn a_drag_lifted_on_another_slot_without_a_move_reaches_it() {
    let (mut touch, layout, now) = dealing(empty(4));
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, C, empty(5)), now),
        [even(1, &[(4, B), (5, C)])]
    );
}

#[test]
fn a_dealing_finger_that_rolls_into_a_gap_still_taps() {
    let (mut touch, layout, now) = dealing(empty(4));
    let gap = [B[0] + 45.0, B[1] + 10.0];
    touch.on(&layout, event(2, Phase::Move, gap, Here::default()), now);
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, gap, Here::default()), now),
        [one(1, 4, B)]
    );
}

#[test]
fn a_drag_passes_over_the_source_and_what_takes_nothing() {
    let (mut touch, layout, now) = dealing(empty(4));
    let path = [
        slot(3),
        slot_of(200, true, SlotKind::Result),
        slot_of(901, true, SlotKind::Display),
        slot_of(TRASH_ID, false, SlotKind::Trash),
        spot(Spot::Outside),
        Here::default(),
    ];
    for (i, over) in (1..).zip(path) {
        let at = [B[0], B[1] + 50.0 * f64::from(i)];
        touch.on(&layout, event(2, Phase::Move, at, over), now);
    }
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, C, empty(5)), now),
        [even(1, &[(4, B), (5, C)])]
    );
}

#[test]
fn a_tap_on_what_takes_nothing_sends_nothing_and_shows_a_cross() {
    for over in [
        slot(3),
        slot_of(200, true, SlotKind::Result),
        slot_of(901, true, SlotKind::Display),
        slot_of(TRASH_ID, false, SlotKind::Trash),
        spot(Spot::Outside),
        spot(Spot::Field),
        Here::default(),
    ] {
        let (mut touch, layout, now) = dealing(over);
        assert!(!marked_above(&touch, &layout, B, now), "{over:?}");
        assert_eq!(
            touch.on(&layout, event(2, Phase::End, B, over), now),
            [],
            "{over:?}"
        );
        assert!(marked_above(&touch, &layout, B, now), "{over:?}");
        assert!(
            !marked_above(&touch, &layout, B, ms(now, 1000)),
            "{over:?}: briefly"
        );
        assert_eq!(
            touch.on(&layout, event(3, Phase::Start, C, empty(5)), now),
            []
        );
        assert_eq!(
            touch.on(&layout, event(3, Phase::End, C, empty(5)), now),
            [one(1, 5, C)],
            "{over:?}: the deal goes on"
        );
    }
}

#[test]
fn a_drag_over_nothing_that_takes_any_sends_nothing() {
    let (mut touch, layout, now) = dealing(Here::default());
    touch.on(&layout, event(2, Phase::Move, C, spot(Spot::Outside)), now);
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, C, spot(Spot::Outside)), now),
        []
    );
}

#[test]
fn a_stroke_in_flight_lands_when_the_resting_finger_lifts_first_in_either_order() {
    for a_first in [true, false] {
        let (mut touch, layout, now) = dealing(empty(4));
        let lift_a = |touch: &mut Touch| touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
        let lift_b = |touch: &mut Touch| touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
        let out = if a_first {
            [lift_a(&mut touch), lift_b(&mut touch)].concat()
        } else {
            [lift_b(&mut touch), lift_a(&mut touch)].concat()
        };
        assert_eq!(out, [one(1, 4, B), end(1)], "A first: {a_first}");
        assert_eq!(touch.deal_view(&layout), None);
    }
}

#[test]
fn no_finger_deals_once_the_resting_one_lifted() {
    let (mut touch, layout, now) = dealing(empty(4));
    touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
    assert!(
        touch
            .on(&layout, event(3, Phase::Start, C, empty(5)), now)
            .is_empty()
    );
    assert!(
        touch
            .on(&layout, event(3, Phase::End, C, empty(5)), now)
            .is_empty()
    );
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        [one(1, 4, B), end(1)]
    );
}

#[test]
fn a_cancel_of_the_resting_finger_ends_the_deal_and_its_stroke_with_no_click() {
    let (mut touch, layout, now) = dealing(empty(4));
    touch.on(&layout, event(2, Phase::Move, C, empty(5)), now);
    assert_eq!(
        touch.on(&layout, event(1, Phase::Cancel, A, slot(3)), now),
        [end(1)]
    );
    assert!(
        touch
            .on(&layout, event(2, Phase::End, C, empty(5)), now)
            .is_empty()
    );
}

#[test]
fn a_cancel_of_the_resting_finger_after_it_lifted_first_ends_with_no_click() {
    let (mut touch, layout, now) = dealing(empty(4));
    touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
    assert_eq!(
        touch.on(&layout, event(2, Phase::Cancel, B, empty(4)), now),
        [end(1)]
    );
}

#[test]
fn a_cancelled_stroke_clicks_nothing_and_the_next_finger_deals_again() {
    let (mut touch, layout, now) = dealing(empty(4));
    touch.on(&layout, event(2, Phase::Move, C, empty(5)), now);
    assert!(
        touch
            .on(&layout, event(2, Phase::Cancel, C, empty(5)), now)
            .is_empty()
    );
    touch.on(&layout, event(3, Phase::Start, D, empty(6)), now);
    assert_eq!(
        touch.on(&layout, event(3, Phase::End, D, empty(6)), now),
        [one(1, 6, D)]
    );
}

#[test]
fn a_scene_change_ends_the_deal_and_its_fingers_click_nothing() {
    let (mut touch, layout, now) = dealing(empty(4));
    let next = next_scene(&touch);
    assert_eq!(touch.frame(next, now), [end(1)]);
    for (id, at, here) in [(2, B, empty(4)), (1, A, slot(3))] {
        assert!(
            touch
                .touch(&layout, next, event(id, Phase::End, at, here), now)
                .is_empty()
        );
    }
}

#[test]
fn a_third_finger_does_nothing_and_shift_changes_no_stroke() {
    let (mut touch, layout, now) = dealing(empty(4));
    assert!(
        touch
            .on(&layout, event(3, Phase::Start, C, empty(5)), now)
            .is_empty()
    );
    assert!(
        touch
            .on(&layout, event(3, Phase::End, C, empty(5)), now)
            .is_empty()
    );
    tap_shift(&mut touch, &layout, now);
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        [one(1, 4, B)]
    );
}

#[test]
fn a_finger_on_shift_during_a_deal_leaves_the_latch_as_it_was() {
    let (mut touch, layout, now) = dealing(empty(4));
    tap_shift(&mut touch, &layout, now);
    touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
    touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
    assert!(!tap_shifted(&mut touch, &layout, 5, slot(3), now));
    let (mut touch, layout, now) = inventory();
    tap_shift(&mut touch, &layout, now);
    touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
    let out = touch.on(&layout, event(2, Phase::Start, B, empty(4)), now);
    answer(&mut touch, &out, Dealt::Done, now);
    tap_shift(&mut touch, &layout, now);
    touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
    touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
    assert!(
        tap_shifted(&mut touch, &layout, 5, slot(3), now),
        "the latch a deal began under stays for the next tap"
    );
}

#[test]
fn shift_stays_unchanged_after_a_deal_ends_until_the_resting_finger_lifts() {
    for taps in 0..=2 {
        // A refused Begin, a source mismatch, or a failed click leaving a carried stack.
        for (refused, stranded) in [(true, false), (false, false), (false, true)] {
            let (mut touch, layout, now) = inventory();
            for _ in 0..taps {
                tap_shift(&mut touch, &layout, now);
            }
            let latch = touch.menus.shift.latch;
            touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
            let begin = touch.on(&layout, event(2, Phase::Start, B, empty(4)), now);
            answer(
                &mut touch,
                &begin,
                if refused { Dealt::Refused } else { Dealt::Done },
                now,
            );
            let stroke = touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
            if !refused {
                answer(&mut touch, &stroke, Dealt::Over, now);
                hand(&mut touch, stranded, now);
            }
            tap_shift(&mut touch, &layout, now);
            assert_eq!(touch.menus.shift.latch, latch);
            assert!(!touch.menus.shift.held);
            touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
            assert_eq!(tap_shifted(&mut touch, &layout, 5, slot(3), now), taps > 0);
            assert_eq!(tap_shifted(&mut touch, &layout, 6, slot(3), now), taps == 2);
            // Shift becomes available again once the resting finger lifts.
            tap_shift(&mut touch, &layout, now);
            assert_eq!(tap_shifted(&mut touch, &layout, 7, slot(3), now), taps != 2);
        }
    }
}

#[test]
fn back_ends_the_deal_then_leaves() {
    let (mut touch, layout, now) = dealing(empty(4));
    let [x, y, w, h] = layout.button(Button::Back);
    let back = [f64::from(x + w / 2.0), f64::from(y + h / 2.0)];
    assert_eq!(
        touch.on(&layout, event(3, Phase::Start, back, Here::default()), now),
        [end(1), Input::Back]
    );
    assert!(
        touch
            .on(&layout, event(2, Phase::End, B, empty(4)), now)
            .is_empty()
    );
}

#[test]
fn a_finger_that_left_its_stack_before_the_second_landed_is_no_source() {
    let (mut touch, layout, now) = inventory();
    touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
    touch.on(&layout, event(1, Phase::Move, B, empty(4)), now);
    assert!(
        touch
            .on(&layout, event(2, Phase::Start, C, empty(5)), now)
            .is_empty()
    );
    assert!(
        touch
            .on(&layout, event(2, Phase::End, C, empty(5)), now)
            .is_empty()
    );
}

#[test]
fn the_resting_finger_moving_moves_the_loupe_and_keeps_its_stack() {
    let (mut touch, layout, now) = dealing(empty(4));
    let before = touch.deal_view(&layout).unwrap().loupe.unwrap();
    let away = [A[0] - 300.0, A[1]];
    assert!(
        touch
            .on(&layout, event(1, Phase::Move, away, empty(7)), now)
            .is_empty()
    );
    let view = touch.deal_view(&layout).unwrap();
    assert_eq!(view.loupe.unwrap()[0], before[0] - 300.0);
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        [one(1, 4, B)]
    );
}

#[test]
fn the_loupe_shows_above_the_resting_finger_and_the_drag_so_far() {
    let (mut touch, layout, now) = dealing(empty(4));
    let view = touch.deal_view(&layout).unwrap();
    let [x, y, w, h] = view.loupe.unwrap();
    assert!(
        x < A[0] as f32 && A[0] as f32 <= x + w,
        "over the finger: {view:?}"
    );
    assert!(
        y + h <= A[1] as f32 - layout.finger_clearance(),
        "clear of the fingertip: {view:?}"
    );
    assert!(w >= 18.0 * layout.gui, "an item fits: {view:?}");
    assert!(view.drag.is_empty());
    touch.on(&layout, event(2, Phase::Move, C, empty(5)), now);
    let targets = |touch: &Touch| touch.deal_view(&layout).unwrap().drag;
    assert_eq!(
        targets(&touch),
        [SlotPoint { slot: 4, at: B }, SlotPoint { slot: 5, at: C }]
    );
    touch.on(&layout, event(1, Phase::End, A, slot(3)), now);
    assert_eq!(touch.deal_view(&layout).unwrap().loupe, None);
    assert_eq!(targets(&touch).len(), 2);
}

#[test]
fn a_second_finger_after_the_long_press_armed_still_deals_and_takes_no_half() {
    let (mut touch, layout, now) = inventory();
    touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
    assert!(tick(&mut touch, ms(now, 2000)).is_empty());
    let out = touch.on(&layout, event(2, Phase::Start, B, empty(4)), ms(now, 2000));
    assert_eq!(out, [begin(1)]);
    answer(&mut touch, &out, Dealt::Done, now);
    touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
    assert_eq!(
        touch.on(&layout, event(1, Phase::End, A, slot(3)), now),
        [end(1)]
    );
}

#[test]
fn a_second_finger_beside_what_cannot_be_dealt_from_does_nothing() {
    for landed in [
        empty(3),
        slot_of(200, true, SlotKind::Result),
        slot_of(901, true, SlotKind::Display),
        slot_of(TRASH_ID, false, SlotKind::Trash),
        listed(slot(3)),
        spot(Spot::Outside),
        Here::default(),
    ] {
        let (mut touch, layout, now) = inventory();
        touch.on(&layout, event(1, Phase::Start, A, landed), now);
        assert!(
            touch
                .on(&layout, event(2, Phase::Start, B, empty(4)), now)
                .is_empty(),
            "{landed:?}"
        );
        assert!(
            touch
                .on(&layout, event(2, Phase::End, B, empty(4)), now)
                .is_empty()
        );
        assert!(
            touch
                .on(&layout, event(1, Phase::End, A, landed), now)
                .contains(&press(false)),
            "{landed:?}: still a tap"
        );
    }
}

#[test]
fn with_a_stack_in_hand_a_second_finger_does_nothing() {
    let (mut touch, layout, now) = inventory();
    hand(&mut touch, true, now);
    touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
    assert!(
        touch
            .on(&layout, event(2, Phase::Start, B, empty(4)), now)
            .is_empty()
    );
}

#[test]
fn the_shift_latch_shifts_no_deal_and_stays_for_the_next_tap() {
    let (mut touch, layout, now) = inventory();
    tap_shift(&mut touch, &layout, now);
    touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
    let out = touch.on(&layout, event(2, Phase::Start, B, empty(4)), now);
    answer(&mut touch, &out, Dealt::Done, now);
    let lifted = [
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        touch.on(&layout, event(1, Phase::End, A, slot(3)), now),
    ]
    .concat();
    assert_eq!(lifted, [one(1, 4, B), end(1)]);
    assert!(tap_shifted(&mut touch, &layout, 5, slot(3), now));
}

#[test]
fn a_finger_holding_shift_makes_a_second_finger_a_third() {
    let (mut touch, layout, now) = inventory();
    let shift = shift_button(&layout);
    touch.on(&layout, event(9, Phase::Start, shift, Here::default()), now);
    touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
    assert!(
        touch
            .on(&layout, event(2, Phase::Start, B, empty(4)), now)
            .is_empty()
    );
}

#[test]
fn a_refused_deal_shows_a_cross_where_the_second_finger_landed_and_neither_clicks() {
    for armed in [false, true] {
        let (mut touch, layout, now) = inventory();
        touch.on(&layout, event(1, Phase::Start, A, slot(3)), now);
        if armed {
            tick(&mut touch, ms(now, 600));
        }
        let out = touch.on(&layout, event(2, Phase::Start, B, empty(4)), now);
        assert_eq!(out, [begin(1)], "armed: {armed}: the pointer stays");
        answer(&mut touch, &out, Dealt::Refused, now);
        assert_eq!(touch.deal_view(&layout), None);
        assert!(marked_above(&touch, &layout, B, now), "armed: {armed}");
        assert!(
            touch
                .on(&layout, event(2, Phase::End, B, empty(4)), now)
                .is_empty()
        );
        assert!(tick(&mut touch, ms(now, 2000)).is_empty());
        assert_eq!(
            touch.on(&layout, event(1, Phase::End, A, slot(3)), now),
            [Input::Pointer(A), Input::Pointer(AWAY)],
            "armed: {armed}: no tap, no half"
        );
    }
}

#[test]
fn a_refused_stroke_shows_a_cross_where_it_dealt_and_the_deal_goes_on() {
    let (mut touch, layout, now) = dealing(empty(4));
    let out = touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
    assert_eq!(out, [one(1, 4, B)]);
    assert!(!marked_above(&touch, &layout, B, now));
    answer(&mut touch, &out, Dealt::Refused, now);
    assert!(marked_above(&touch, &layout, B, now));
    touch.on(&layout, event(3, Phase::Start, C, empty(5)), now);
    assert_eq!(
        touch.on(&layout, event(3, Phase::End, C, empty(5)), now),
        [one(2, 5, C)]
    );
}

#[test]
fn a_stroke_that_ends_the_deal_spends_every_finger_until_the_resting_one_lifts() {
    // `stranded`: a click did other than planned and left a stack in hand.
    for stranded in [false, true] {
        let (mut touch, layout, now) = dealing(empty(4));
        assert!(marked_above(&touch, &layout, A, now), "the loupe's plate");
        let out = touch.on(&layout, event(2, Phase::End, B, empty(4)), now);
        answer(&mut touch, &out, Dealt::Over, now);
        hand(&mut touch, stranded, now);
        assert!(
            marked_above(&touch, &layout, B, now),
            "a cross where it dealt"
        );
        assert!(!marked_above(&touch, &layout, A, now), "no plate");
        assert_eq!(touch.deal_view(&layout), None);
        assert_eq!(touch.rest(), Some(B), "a stack left in hand shows there");
        assert_eq!(
            touch.on(&layout, event(1, Phase::Move, D, empty(6)), now),
            [Input::Pointer(D)],
            "the resting finger only hovers"
        );
        let later = ms(now, 2000);
        for id in [3, 4] {
            assert!(
                touch
                    .on(&layout, event(id, Phase::Start, C, slot(5)), now)
                    .is_empty()
            );
            assert!(tick(&mut touch, later).is_empty(), "no half");
            assert!(
                touch
                    .on(&layout, event(id, Phase::End, C, slot(5)), later)
                    .is_empty(),
                "stranded: {stranded}: no pickup while the resting finger stays"
            );
        }
        assert_eq!(
            touch.on(&layout, event(1, Phase::End, D, empty(6)), later),
            [Input::Pointer(D), Input::Pointer(AWAY)]
        );
        assert_eq!(
            touch.on(&layout, event(5, Phase::Start, C, slot(5)), later),
            [Input::Pointer(C)]
        );
        assert_eq!(
            touch.on(&layout, event(5, Phase::End, C, slot(5)), later),
            [
                Input::Pointer(C),
                press(false),
                Input::Release(LEFT),
                Input::Pointer(AWAY)
            ],
            "stranded: {stranded}: an ordinary tap once it lifted"
        );
    }
}

#[test]
fn a_finger_that_starts_again_under_the_same_id_ends_what_it_did() {
    let (mut touch, layout, now) = dealing(empty(4));
    assert_eq!(
        touch.on(&layout, event(1, Phase::Start, A, slot(3)), now),
        [end(1), Input::Pointer(A)]
    );
    let (mut touch, layout, now) = dealing(empty(4));
    touch.on(&layout, event(2, Phase::Move, C, empty(5)), now);
    assert!(
        touch
            .on(&layout, event(2, Phase::Start, D, empty(6)), now)
            .is_empty()
    );
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, D, empty(6)), now),
        [one(1, 6, D)],
        "the stroke before it clicks nothing"
    );
}

#[test]
fn an_answer_to_no_question_changes_nothing() {
    let (mut touch, layout, now) = dealing(empty(4));
    let other = Deal::Begin {
        deal: 7,
        source: SOURCE,
    };
    touch.dealt(&other, Dealt::Refused, now);
    assert_eq!(
        touch.on(&layout, event(2, Phase::End, B, empty(4)), now),
        [one(1, 4, B)]
    );
}
