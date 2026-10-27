//! A deal from a stored stack: one of its items on another slot, or an even share of it on
//! several, and the rest put back where it was, through vanilla's own clicks. A left click picks
//! the stack up, a right click (one item) or a left drag (QUICK_CRAFT type 0) places it, and a
//! left click puts the rest back. The clicks are planned on a copy of the menu, so the game sends
//! them only when they move exactly that, and checks each stage as it replays them with the
//! mouse (the touch controls' two-finger deal).

use super::{Env, GRID, Input, Kind, MAIN, Menu, PLAYER, Role};
use crate::bundle::Contents;
use crate::items::ItemStack;

/// How a deal shares its stack over its targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// One item on the one target: the right button's click there (PICKUP 1).
    One,
    /// The stack shared evenly over the targets: vanilla's left drag, or the left button's click
    /// when only one target takes it.
    Even,
}

/// What the hand and a deal's slots hold after one of its clicks: the source, then each target.
#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    pub carried: Option<ItemStack>,
    pub slots: Vec<Option<ItemStack>>,
}

/// A deal's clicks, proved on a copy of the menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// What places the stack: the right button's click, or the left button's click or drag.
    pub split: Split,
    /// The targets asked for that the stack goes to, as positions in that list, in order: the one
    /// of a `Split::One`, the slots vanilla's drag collects for a `Split::Even`.
    pub targets: Vec<usize>,
    /// After the stack is picked up.
    pub picked: Stage,
    /// After it is placed.
    pub placed: Stage,
    /// After what is left is put back: None when nothing is left in hand.
    pub returned: Option<Stage>,
}

impl Plan {
    /// What a drag shows before it lifts, as vanilla's drag previews the stack in hand: each
    /// target's count once placed, and whether its slot holds less than its share (a yellow
    /// count); then what goes back to the source. A drag that reaches one slot shows the whole
    /// stack going there, as the left button's click puts it.
    pub fn preview(&self) -> (Vec<(u32, bool)>, u32) {
        let count = |stack: &Option<ItemStack>| stack.as_ref().map_or(0, |s| s.count);
        let share = count(&self.picked.carried) / self.targets.len() as u32;
        let shown = self.picked.slots[1..]
            .iter()
            .zip(&self.placed.slots[1..])
            .map(|(was, now)| (count(now), count(now) < count(was) + share))
            .collect();
        (shown, count(&self.placed.carried))
    }
}

impl Menu {
    /// The clicks that deal from the stack in slot `source` onto `targets` (in the order a finger
    /// reached them), proved on a copy of this menu: they leave the hand empty, the stack's items
    /// only in the source and the targets that take some, and every other slot as it was but the
    /// results those change. None: the source can't be dealt from, or no target takes any.
    pub fn plan_deal(
        &self,
        env: &Env,
        source: usize,
        targets: &[usize],
        split: Split,
    ) -> Option<Plan> {
        if !self.deals_from(env, source) {
            return None;
        }
        let stack = self.slot(source)?.clone();
        let mut menu = self.clone();
        menu.click(env, source as i32, 0, Input::Pickup).ok()?;
        if menu.carried.as_ref() != Some(&stack) || menu.slot(source).is_some() {
            return None;
        }
        let chosen = menu.deal_targets(env, source, targets, split);
        let slots: Vec<usize> = chosen.iter().map(|&i| targets[i]).collect();
        if slots.is_empty() {
            return None;
        }
        let stage = |menu: &Menu| Stage {
            carried: menu.carried.clone(),
            slots: std::iter::once(source)
                .chain(slots.iter().copied())
                .map(|s| menu.slot(s).cloned())
                .collect(),
        };
        let picked = stage(&menu);
        match (split, &slots[..]) {
            (Split::One, &[t]) => menu.click(env, t as i32, 1, Input::Pickup).ok()?,
            (Split::Even, &[t]) => menu.click(env, t as i32, 0, Input::Pickup).ok()?,
            (Split::Even, slots) => {
                menu.click(env, -999, 0, Input::QuickCraft).ok()?;
                for &t in slots {
                    menu.click(env, t as i32, 1, Input::QuickCraft).ok()?;
                }
                menu.click(env, -999, 2, Input::QuickCraft).ok()?;
            }
            (Split::One, _) => return None,
        }
        let placed = stage(&menu);
        let returned = match menu.carried {
            Some(_) => {
                menu.click(env, source as i32, 0, Input::Pickup).ok()?;
                Some(stage(&menu))
            }
            None => None,
        };
        let dealt = std::iter::once(source).chain(slots.iter().copied());
        let count = |m: &Menu| {
            dealt
                .clone()
                .map(|s| m.slot(s).map_or(0, |i| i.count))
                .sum::<u32>()
        };
        let conserved = count(&menu) == count(self)
            && dealt.clone().all(|s| {
                menu.slot(s)
                    .is_none_or(|i| i.same_item_same_components(&stack))
            });
        let rest_kept = (0..self.slots.len())
            .filter(|&s| s != source && !slots.contains(&s) && !self.slots[s].role.output())
            .all(|s| menu.slot(s) == self.slot(s));
        (menu.carried.is_none() && conserved && rest_kept).then_some(Plan {
            split,
            targets: chosen,
            picked,
            placed,
            returned,
        })
    }

    /// Whether a deal may take the stack in slot `s`, under an empty hand, and put back what is
    /// left: a stack stored in the player's inventory, a crafting grid or a container that only
    /// stores. Not a bundle, whose clicks move items in and out of it; not armour, the off hand or
    /// a mount's equipment, whose changes equip; not a station's input, whose change resets what
    /// the station does (a stonecutter's chosen recipe, a furnace's progress).
    fn deals_from(&self, env: &Env, s: usize) -> bool {
        let Some(slot) = self.slots.get(s) else {
            return false;
        };
        let stored = match slot.container {
            PLAYER => self.equipment_slot(s).is_none(),
            GRID => true,
            MAIN => matches!(
                self.kind,
                Kind::Chest(_) | Kind::Hopper | Kind::Dispenser | Kind::Shulker | Kind::Mount(_)
            ),
            _ => false,
        };
        stored
            && matches!(slot.role, Role::Plain | Role::Shulker)
            && self.carried.is_none()
            && self
                .slot(s)
                .is_some_and(|stack| Contents::of(env.items, stack).is_none())
    }

    /// Which of `targets` the carried stack goes to, as positions in that list: for one item, the
    /// one target if the right click puts exactly one there; for an even share, each new slot
    /// with room that vanilla's left drag collects, in order (`shouldAddSlotToQuickCraft`).
    fn deal_targets(
        &self,
        env: &Env,
        source: usize,
        targets: &[usize],
        split: Split,
    ) -> Vec<usize> {
        let Some(carried) = self.carried.as_ref() else {
            return Vec::new();
        };
        match (split, targets) {
            (Split::One, &[t]) if t != source && self.would_place_one(env, t) => vec![0],
            (Split::One, _) => Vec::new(),
            (Split::Even, _) => {
                let mut chosen: Vec<usize> = Vec::new();
                for (i, &t) in targets.iter().enumerate() {
                    let new = t != source && chosen.iter().all(|&c| targets[c] != t);
                    // A full stack takes no share of vanilla's drag, but lowers the others'.
                    let room = t < self.slots.len()
                        && self.slot(t).is_none_or(|have| {
                            have.count
                                < Self::max_stack(env, carried).min(self.slot_max(env, t, carried))
                        });
                    if new && room && self.quick_craft_accepts(env, t, 0, chosen.len()) {
                        chosen.push(i);
                    }
                }
                chosen
            }
        }
    }
}

#[cfg(test)]
mod tests;
