//! Two fingers on an inventory: one rests on a stack (the source) and the other deals from it. A
//! tap puts one item on the slot tapped, a drag shares the stack evenly over the slots it crosses
//! (vanilla's left drag), and what is left goes back to the source. Each stroke of the dealing
//! finger is one `Deal::Commit`, which the game answers at once (`Touch::dealt`): its clicks go out
//! together or not at all, so the hand is empty between strokes and the resting finger never
//! clicks. A click that does other than planned stops the rest and ends the deal.

use super::super::{Input, Role, Touch};
use super::{Gesture, Here, Layout, Pointer, SlotKind, Spot, left};
use std::time::{Duration, Instant};

/// How long the cross shows where a deal took nothing.
pub(super) const REFUSAL: Duration = Duration::from_millis(300);
/// The loupe's side in GUI pixels: an item and its count.
const LOUPE: f32 = 18.0;

/// A slot and a point inside it, in physical pixels: where the game clicks it, once it has
/// checked that the slot is still under the point (a fold or a resize moves slots).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotPoint {
    pub slot: u32,
    pub at: [f64; 2],
}

/// What a dealing finger's stroke asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Share {
    /// A tap: one item on this slot.
    One(SlotPoint),
    /// A drag: the stack shared evenly over the slots it reached, in order.
    Even(Vec<SlotPoint>),
}

impl Share {
    /// Where the stroke dealt first.
    fn first(&self) -> Option<SlotPoint> {
        match self {
            Share::One(target) => Some(*target),
            Share::Even(targets) => targets.first().copied(),
        }
    }
}

/// A two-finger deal, as the game is asked for it. Each is answered at once (`Touch::dealt`).
#[derive(Clone, Debug, PartialEq)]
pub enum Deal {
    /// A finger landed while another rests on the stack in `source`, under an empty hand: may it
    /// deal from there? Nothing is clicked. Begun, the pointer hovers away, as the source's
    /// tooltip would cover the loupe.
    Begin { deal: u64, source: SlotPoint },
    /// The dealing finger's stroke number `stroke` lifted: the source's stack picked up, `share`
    /// of it placed and the rest put back, the mouse's own clicks, sent only if they do exactly
    /// that. On a server each click is its own packet, predicted at once.
    Commit {
        deal: u64,
        stroke: u64,
        share: Share,
    },
    /// The deal is over: nothing is clicked.
    End { deal: u64 },
}

/// The game's answer to a `Deal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dealt {
    /// The deal may begin, or the stroke's clicks went out.
    Done,
    /// Nothing was clicked: the source can't be dealt from, the slot takes none of it (another
    /// item, a full stack, a bundle), a slot moved, or the screen changed.
    Refused,
    /// At the stroke's source check, its item, components or count differ from what the deal
    /// left there, and nothing was clicked; or a click did other than planned, and no more went
    /// out. The deal is over: no finger does anything more until the resting one lifts.
    Over,
}

/// What the game draws of a deal: the source's stack on the loupe, a square above the resting
/// finger (physical pixels), which covers the source; and the slots a drag reached so far.
#[derive(Clone, Debug, PartialEq)]
pub struct DealView {
    /// None once the resting finger lifted, while a stroke is still down.
    pub loupe: Option<[f32; 4]>,
    pub drag: Vec<SlotPoint>,
}

/// The pointer while one finger rests on a stack and another deals from it.
#[derive(Debug)]
pub(in crate::touch) struct Dealing {
    /// The game's answers and the strokes name the deal by it.
    id: u64,
    source: SlotPoint,
    fingers: Fingers,
    /// The strokes sent so far.
    strokes: u64,
}

#[derive(Debug)]
enum Fingers {
    /// The resting finger, where it is now (the loupe follows it), and the dealing finger, while
    /// one is down.
    Held {
        holder: u64,
        at: [f64; 2],
        dealer: Option<Dealer>,
    },
    /// The resting finger lifted while a stroke was down: the stroke still lands, then the deal
    /// is over. Two thumbs lifting together come up in either order.
    Last(Dealer),
}

/// A dealing finger's stroke.
#[derive(Debug)]
struct Dealer {
    finger: u64,
    from: [f64; 2],
    landed: Here,
    path: Path,
}

#[derive(Debug)]
enum Path {
    /// Still where it landed: a tap.
    Tap,
    /// It left: the slots it reached, in order, each once, and never the source.
    Drag(Vec<SlotPoint>),
}

/// The stack a pointer finger resting as `gesture` can be dealt from: in a slot, where it
/// landed, before it pressed or scrolled anything. A long press armed on it is forgotten.
fn source(gesture: &Gesture) -> Option<SlotPoint> {
    let (Gesture::Pending { from, here, .. } | Gesture::Splitting { from, landed: here }) = gesture
    else {
        return None;
    };
    match (here.spot, here.list) {
        (Spot::Slot(s), None) if s.filled && s.kind == SlotKind::Normal => Some(SlotPoint {
            slot: s.id,
            at: *from,
        }),
        _ => None,
    }
}

/// The slot under `here`, at `at`, as a stroke's target: a plain slot other than the source.
fn target(source: u32, at: [f64; 2], here: &Here) -> Option<SlotPoint> {
    match here.spot {
        Spot::Slot(s) if s.kind == SlotKind::Normal && s.id != source => {
            Some(SlotPoint { slot: s.id, at })
        }
        _ => None,
    }
}

impl Dealer {
    /// It moved to `at`, over `here`. Leaving where it landed (`left`, with the gap rule of a
    /// stack in hand) makes it a drag, which reaches the slot it landed on first.
    fn slide(&mut self, source: u32, at: [f64; 2], here: &Here, layout: &Layout) {
        if let Path::Tap = self.path {
            if !left(&self.landed, self.from, here, at, layout, true) {
                return;
            }
            self.path = Path::Drag(
                target(source, self.from, &self.landed)
                    .into_iter()
                    .collect(),
            );
        }
        if let Path::Drag(path) = &mut self.path
            && let Some(slot) = target(source, at, here)
            && path.iter().all(|p| p.slot != slot.slot)
        {
            path.push(slot);
        }
    }

    /// What its stroke asks for, if anything.
    fn share(self, source: u32) -> Option<Share> {
        match self.path {
            Path::Tap => target(source, self.from, &self.landed).map(Share::One),
            Path::Drag(path) if !path.is_empty() => Some(Share::Even(path)),
            Path::Drag(_) => None,
        }
    }
}

impl Dealing {
    pub(super) fn id(&self) -> u64 {
        self.id
    }

    /// Finger `finger` moved to `at`, over `here`: the resting finger moves the loupe only; the
    /// source stays the slot it landed on.
    pub(super) fn slide(&mut self, finger: u64, at: [f64; 2], here: &Here, layout: &Layout) {
        let source = self.source.slot;
        match &mut self.fingers {
            Fingers::Held {
                holder, at: rests, ..
            } if *holder == finger => *rests = at,
            Fingers::Held {
                dealer: Some(dealer),
                ..
            }
            | Fingers::Last(dealer)
                if dealer.finger == finger =>
            {
                dealer.slide(source, at, here, layout)
            }
            _ => {}
        }
    }
}

impl Touch {
    /// A finger lands beside the pointer's (`land_menu`, after Back and Shift). Some: it deals, or
    /// it is a finger too many. None: the pointer has no deal for it.
    pub(super) fn land_deal(
        &mut self,
        finger: u64,
        at: [f64; 2],
        here: Here,
        out: &mut Vec<Input>,
    ) -> Option<Role> {
        let dealer = Dealer {
            finger,
            from: at,
            landed: here,
            path: Path::Tap,
        };
        let (pointer, role) = match std::mem::take(&mut self.menus.pointer) {
            Pointer::Single {
                finger: holder,
                gesture,
            } => match source(&gesture) {
                // A finger holding Shift down makes this one a third.
                Some(source) if !self.context.carrying && !self.menus.shift.held => {
                    self.menus.deals += 1;
                    let deal = Dealing {
                        id: self.menus.deals,
                        source,
                        fingers: Fingers::Held {
                            holder,
                            at: source.at,
                            dealer: Some(dealer),
                        },
                        strokes: 0,
                    };
                    out.push(Input::Deal(Deal::Begin {
                        deal: deal.id,
                        source,
                    }));
                    let asking = Pointer::Asking {
                        finger: holder,
                        landed: at,
                        deal,
                    };
                    (asking, Some(Role::Menu))
                }
                _ => (
                    Pointer::Single {
                        finger: holder,
                        gesture,
                    },
                    None,
                ),
            },
            Pointer::Dealing(mut deal) => {
                let role = match &mut deal.fingers {
                    Fingers::Held { dealer: next, .. } if next.is_none() => {
                        *next = Some(dealer);
                        Role::Menu
                    }
                    _ => Role::Spent,
                };
                (Pointer::Dealing(deal), Some(role))
            }
            Pointer::Idle => (Pointer::Idle, None),
            asking @ Pointer::Asking { .. } => (asking, Some(Role::Spent)),
        };
        self.menus.pointer = pointer;
        role
    }

    /// Finger `finger` of a deal lifts at `at`, over `here`. The dealing finger's stroke is sent,
    /// or refused with a cross; the deal ends once neither finger is down.
    pub(super) fn lift_deal(
        &mut self,
        mut deal: Dealing,
        finger: u64,
        at: [f64; 2],
        here: &Here,
        layout: &Layout,
        now: Instant,
        out: &mut Vec<Input>,
    ) {
        // A stroke a click stopped halfway leaves its stack in hand, drawn here.
        self.menus.rest = Some(at);
        // The lift's point is the stroke's last, whether or not a move came there first.
        deal.slide(finger, at, here, layout);
        let (dealer, rest) = match deal.fingers {
            Fingers::Held {
                holder,
                dealer: Some(dealer),
                ..
            } if holder == finger => (None, Some(Fingers::Last(dealer))),
            Fingers::Held {
                holder,
                dealer: None,
                ..
            } if holder == finger => (None, None),
            Fingers::Held {
                holder,
                at: rests,
                dealer: Some(dealer),
            } if dealer.finger == finger => (
                Some(dealer),
                Some(Fingers::Held {
                    holder,
                    at: rests,
                    dealer: None,
                }),
            ),
            Fingers::Last(dealer) if dealer.finger == finger => (Some(dealer), None),
            fingers => (None, Some(fingers)),
        };
        if let Some(dealer) = dealer {
            match dealer.share(deal.source.slot) {
                Some(share) => {
                    deal.strokes += 1;
                    out.push(Input::Deal(Deal::Commit {
                        deal: deal.id,
                        stroke: deal.strokes,
                        share,
                    }));
                }
                None => self.menus.refused = Some((at, now)),
            }
        }
        match rest {
            Some(fingers) => {
                deal.fingers = fingers;
                self.menus.pointer = Pointer::Dealing(deal);
            }
            None => out.push(Input::Deal(Deal::End { deal: deal.id })),
        }
    }

    /// Finger `finger` of a deal cancelled: the dealing finger's stroke goes, clicking nothing;
    /// the resting finger's ends the deal, and the stroke in flight with it.
    pub(super) fn cancel_deal(&mut self, mut deal: Dealing, finger: u64, out: &mut Vec<Input>) {
        deal.fingers = match deal.fingers {
            Fingers::Held {
                holder,
                at,
                dealer: Some(dealer),
            } if dealer.finger == finger => Fingers::Held {
                holder,
                at,
                dealer: None,
            },
            Fingers::Held { holder, .. } if holder == finger => {
                return out.push(Input::Deal(Deal::End { deal: deal.id }));
            }
            Fingers::Last(dealer) if dealer.finger == finger => {
                return out.push(Input::Deal(Deal::End { deal: deal.id }));
            }
            fingers => fingers,
        };
        self.menus.pointer = Pointer::Dealing(deal);
    }

    /// The game's answer to `deal`, given before the next event or frame. A deal the game refuses
    /// to begin shows a cross where the second finger landed, and neither finger does more: a
    /// two-finger attempt never becomes the resting finger's own tap or half. A stroke that
    /// clicked nothing shows a cross where it dealt; one that ends the deal leaves the resting
    /// finger only hovering, and every other finger does nothing until it lifts: nor does a
    /// two-finger attempt become another finger's pickup. An answer to no question does nothing.
    pub fn dealt(&mut self, deal: &Deal, answer: Dealt, now: Instant) {
        match deal {
            Deal::Begin { deal: id, .. } => {
                self.menus.pointer = match std::mem::take(&mut self.menus.pointer) {
                    Pointer::Asking { deal, .. } if deal.id == *id && answer == Dealt::Done => {
                        Pointer::Dealing(deal)
                    }
                    Pointer::Asking {
                        finger,
                        landed,
                        deal,
                    } if deal.id == *id => {
                        self.menus.refused = Some((landed, now));
                        Pointer::Single {
                            finger,
                            gesture: Gesture::EndedDeal,
                        }
                    }
                    pointer => pointer,
                }
            }
            Deal::Commit {
                deal: id, share, ..
            } if answer != Dealt::Done => {
                if let Some(target) = share.first() {
                    self.menus.refused = Some((target.at, now));
                }
                if answer == Dealt::Over
                    && let Pointer::Dealing(Dealing {
                        id: dealing,
                        fingers: Fingers::Held { holder, .. },
                        ..
                    }) = self.menus.pointer
                    && dealing == *id
                {
                    self.menus.pointer = Pointer::Single {
                        finger: holder,
                        gesture: Gesture::EndedDeal,
                    };
                }
            }
            Deal::Commit { .. } | Deal::End { .. } => {}
        }
    }

    /// What the game draws of the deal in progress, if one is.
    pub fn deal_view(&self, layout: &Layout) -> Option<DealView> {
        let Pointer::Dealing(deal) = &self.menus.pointer else {
            return None;
        };
        let (loupe, dealer) = match &deal.fingers {
            Fingers::Held { at, dealer, .. } => (Some(loupe(layout, *at)), dealer.as_ref()),
            Fingers::Last(dealer) => (None, Some(dealer)),
        };
        let drag = match dealer.map(|d| &d.path) {
            Some(Path::Drag(path)) => path.clone(),
            _ => Vec::new(),
        };
        Some(DealView { loupe, drag })
    }
}

/// The loupe over a finger at `at`: as big as `Layout::above`'s square or an item's, whichever is
/// larger, on the same spot.
fn loupe(layout: &Layout, at: [f64; 2]) -> [f32; 4] {
    let [x, y, w, h] = layout.above([at[0] as f32, at[1] as f32]);
    let side = w.max(LOUPE * layout.gui);
    [
        (x + (w - side) / 2.0).max(0.0),
        (y + h - side).max(0.0),
        side,
        side,
    ]
}
