//! The touch controls' two-finger deal (`horizon_client::touch::Deal`) applied to an inventory.
//! Each stroke's clicks are planned on a copy of the menu (`Menu::plan_deal`) and sent as the
//! mouse's own presses and releases, each stage read back before the next. They all go out within
//! one touch event, so a stage check catches the mouse's path going otherwise than the engine's
//! plan (a tweak, a screen's own rule). On a server every click is its own packet, predicted here
//! at once, and the server's answers reach the menu only between frames: a change it makes shows
//! at the next stroke's check of the source.

use super::{DragView, Game, Scene};
use horizon_client::{
    hud::HudBatch,
    items::ItemStack,
    keybinds,
    menu::deal::{Plan, Split, Stage},
    touch::{AWAY, Deal, Dealt, Layout, Share, SlotPoint, Spot},
};
use winit::{
    event::{DeviceId, MouseButton},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

/// A deal the game began: what it deals from, checked before every stroke.
pub(super) struct TouchDeal {
    deal: u64,
    scene: Scene,
    source: SlotPoint,
    /// The source's item when the deal began, and how many of it the source holds now.
    stack: ItemStack,
    left: u32,
    /// The last stroke dealt.
    stroke: u64,
    /// What the drag under way would give, worked out once a frame (`preview_deal`).
    preview: Option<DragView>,
}

impl TouchDeal {
    /// Whether the source's current item, components and count match what the deal left there.
    /// A mismatch hides feedback and, if observed by a stroke, ends the deal. This compares
    /// current values, not intervening changes: an identical refill before that check matches.
    fn holds(&self, now: Option<ItemStack>) -> bool {
        let left = (self.left > 0).then(|| ItemStack {
            count: self.left,
            ..self.stack.clone()
        });
        now == left
    }
}

impl Game<'_> {
    /// The answer to `deal`. `allowed`: the batch may click (no world opening or saving, and the
    /// scene is still the one the fingers were on); End is always taken.
    pub(super) fn touch_deal(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        device_id: DeviceId,
        deal: &Deal,
        allowed: bool,
    ) -> Dealt {
        match *deal {
            Deal::End { deal } => {
                if self.touch_deal.as_ref().is_some_and(|d| d.deal == deal) {
                    self.touch_deal = None;
                }
                Dealt::Done
            }
            _ if !allowed => Dealt::Refused,
            Deal::Begin { deal, source } => {
                let answer = self.begin_deal(deal, source);
                // The source's tooltip would cover the loupe.
                if answer == Dealt::Done {
                    self.touch_pointer(event_loop, id, device_id, AWAY);
                }
                answer
            }
            Deal::Commit {
                deal,
                stroke,
                ref share,
            } => {
                let Some(dealing) = self
                    .touch_deal
                    .take_if(|d| d.deal == deal && d.stroke < stroke)
                else {
                    return Dealt::Refused;
                };
                let answer = self.commit_deal(event_loop, id, device_id, dealing, stroke, share);
                self.touch_pointer(event_loop, id, device_id, AWAY);
                answer
            }
        }
    }

    fn begin_deal(&mut self, deal: u64, source: SlotPoint) -> Dealt {
        let slot = source.slot as usize;
        if !self.deal_may_click() || !self.slot_under(source) {
            return Dealt::Refused;
        }
        let mut deals = false;
        self.menu_session(|menu, env, to_menu| {
            deals = to_menu(slot).is_some_and(|s| menu.deals_from(env, s));
            Some(false)
        });
        let Some(stack) = self.slot_view(slot).filter(|_| deals) else {
            return Dealt::Refused;
        };
        self.touch_deal = Some(TouchDeal {
            deal,
            scene: self.scene(),
            source,
            left: stack.count,
            stack,
            stroke: 0,
            preview: None,
        });
        Dealt::Done
    }

    /// Stroke `stroke` of `dealing`: its clicks, if its plan holds. Refused before a click keeps
    /// the deal. Over: the source holds other than the deal left there, or a click did something
    /// else, which stops the deal and leaves whatever it put in hand there.
    fn commit_deal(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        device_id: DeviceId,
        mut dealing: TouchDeal,
        stroke: u64,
        share: &Share,
    ) -> Dealt {
        let (split, targets) = match share {
            Share::One(target) => (Split::One, std::slice::from_ref(target)),
            Share::Even(targets) => (Split::Even, &targets[..]),
        };
        // Its drag lifted.
        dealing.preview = None;
        let source = dealing.source;
        let scene = self.scene() == dealing.scene;
        if scene && !dealing.holds(self.slot_view(source.slot as usize)) {
            return Dealt::Over;
        }
        let ready = scene
            && self.deal_may_click()
            && std::iter::once(&source)
                .chain(targets)
                .all(|&point| self.slot_under(point));
        let planned = ready
            .then(|| self.plan_deal(source.slot, targets, split))
            .flatten();
        let Some((plan, points)) = planned else {
            self.touch_deal = Some(dealing);
            return Dealt::Refused;
        };
        if !self.deal_clicks(event_loop, id, device_id, &dealing, &points, &plan) {
            log::warn!(
                "Horizon: touch: a deal's click did other than planned, so the deal stopped"
            );
            return Dealt::Over;
        }
        dealing.left = self
            .slot_view(source.slot as usize)
            .map_or(0, |stack| stack.count);
        dealing.stroke = stroke;
        self.touch_deal = Some(dealing);
        Dealt::Done
    }

    /// Nothing stands between a deal's clicks and the menu: an open inventory the recipe book
    /// leaves the pointer to, an empty hand, no drag of the mouse's, no Shift held on a keyboard,
    /// no Mouse Tweaks to change what a click does, and no sort bound to a button the deal clicks
    /// with (its press would sort instead).
    fn deal_may_click(&self) -> bool {
        self.inventory_open()
            && !self.recipe_book_takes_pointer()
            && self.carried.is_none()
            && self.inv.drag.is_none()
            && self.inv.clicks.quick_crafting().is_none()
            && !self.has_shift_down()
            && !self.tweaks_on()
            && ![MouseButton::Left, MouseButton::Right]
                .into_iter()
                .any(|button| self.sort_bound_to(&keybinds::mouse_name(button)))
    }

    /// Slot `point.slot` is still the one under `point`, as a finger finds it there: no fold,
    /// unfold or resize moved it.
    fn slot_under(&mut self, point: SlotPoint) -> bool {
        let Some(batch) = self.gui_scale() else {
            return false;
        };
        let p = (
            point.at[0] as f32 / batch.scale,
            point.at[1] as f32 / batch.scale,
        );
        matches!(self.inventory_spot(&batch, p), Some(Spot::Slot(s)) if s.id == point.slot)
    }

    /// The deal from game slot `source` onto `targets`, planned on the open menu's copy, and the
    /// targets it chose.
    fn plan_deal(
        &mut self,
        source: u32,
        targets: &[SlotPoint],
        split: Split,
    ) -> Option<(Plan, Vec<SlotPoint>)> {
        let mut plan = None;
        self.menu_session(|menu, env, to_menu| {
            let slots = targets
                .iter()
                .map(|t| to_menu(t.slot as usize))
                .collect::<Option<Vec<_>>>()?;
            plan = menu.plan_deal(env, to_menu(source as usize)?, &slots, split);
            Some(false)
        });
        let plan = plan?;
        let chosen = plan.targets.iter().map(|&i| targets[i]).collect();
        Some((plan, chosen))
    }

    /// The plan's clicks, as the mouse sends them onto `points` (the targets it chose), each stage
    /// read back. False: one did something else, or the scene changed, and no more went out.
    fn deal_clicks(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        device_id: DeviceId,
        dealing: &TouchDeal,
        points: &[SlotPoint],
        plan: &Plan,
    ) -> bool {
        let slots: Vec<usize> = std::iter::once(dealing.source)
            .chain(points.iter().copied())
            .map(|p| p.slot as usize)
            .collect();
        let reached = |game: &mut Self, stage: &Stage| {
            game.scene() == dealing.scene
                && game.carried == stage.carried
                && slots
                    .iter()
                    .zip(&stage.slots)
                    .all(|(&slot, want)| game.slot_view(slot) == *want)
        };
        let source = dealing.source.at;
        self.touch_press(event_loop, id, device_id, &[source], MouseButton::Left);
        if !reached(self, &plan.picked) {
            return false;
        }
        let at: Vec<[f64; 2]> = points.iter().map(|p| p.at).collect();
        let button = match plan.split {
            Split::One => MouseButton::Right,
            Split::Even => MouseButton::Left,
        };
        self.touch_press(event_loop, id, device_id, &at, button);
        if !reached(self, &plan.placed) {
            return false;
        }
        match &plan.returned {
            Some(returned) => {
                self.touch_press(event_loop, id, device_id, &[source], MouseButton::Left);
                reached(self, returned)
            }
            None => true,
        }
    }

    /// The deal begun, while its source holds what the deal left there (`TouchDeal::holds`).
    fn holding_deal(&mut self) -> Option<&TouchDeal> {
        let source = self.touch_deal.as_ref()?.source.slot as usize;
        let now = self.slot_view(source);
        self.touch_deal
            .as_ref()
            .filter(|dealing| dealing.holds(now))
    }

    /// Once a frame, before it is drawn, and for `screen`: what the deal's drag would give the
    /// slots it reached so far, from the stroke's own plan, which the slots and the loupe both
    /// show. With no drag under way, no menu is built.
    pub(super) fn preview_deal(&mut self) {
        if self.touch_deal.is_none() {
            return;
        }
        let drag = self
            .touch_layout()
            .and_then(|layout| self.touch.deal_view(&layout))
            .map_or_else(Vec::new, |view| view.drag);
        let deal = (!drag.is_empty())
            .then(|| {
                self.holding_deal()
                    .map(|dealing| (dealing.source.slot, dealing.stack.clone()))
            })
            .flatten();
        let preview = deal.and_then(|(source, stack)| {
            let (plan, points) = self.plan_deal(source, &drag, Split::Even)?;
            let (shown, left) = plan.preview();
            Some(DragView {
                slots: points
                    .iter()
                    .map(|p| p.slot as usize)
                    .zip(shown.into_iter().map(Some))
                    .collect(),
                left: Some(left),
                stack,
            })
        });
        if let Some(dealing) = &mut self.touch_deal {
            dealing.preview = preview;
        }
    }

    /// This frame's preview of the deal's drag (`preview_deal`), as vanilla's drag previews the
    /// stack in hand (`DragView`).
    pub(super) fn deal_preview(&self) -> Option<DragView> {
        self.touch_deal.as_ref()?.preview.clone()
    }

    /// The loupe above the finger resting on the source, which covers its slot (its square, in
    /// physical pixels), and the source's stack on it with its count, down to 0: what a drag
    /// would leave while one is under way. None once something else changed the source.
    pub(super) fn deal_loupe(&mut self, layout: &Layout) -> Option<([f32; 4], ItemStack, u32)> {
        let square = self.touch.deal_view(layout)?.loupe?;
        let dealing = self.holding_deal()?;
        let left = dealing.preview.as_ref().and_then(|view| view.left);
        Some((square, dealing.stack.clone(), left.unwrap_or(dealing.left)))
    }

    pub(super) fn draw_deal_loupe(&mut self, batch: &mut HudBatch, layout: &Layout) {
        let Some((square, stack, count)) = self.deal_loupe(layout) else {
            return;
        };
        let [left, top, side, _] = square.map(|v| v / batch.scale);
        let corner = (side - 16.0) / 2.0;
        let (x, y) = (left + corner, top + corner);
        self.draw_item_model(batch, &stack, x, y);
        self.draw_item_decorations(batch, &stack, x, y, Some(count.to_string()));
    }
}
