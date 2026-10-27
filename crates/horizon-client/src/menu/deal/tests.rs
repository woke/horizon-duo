use super::*;
use crate::menu::tests::with_menu;

fn stack(item: &str, count: u32) -> Option<ItemStack> {
    Some(ItemStack {
        item: format!("minecraft:{item}").into(),
        count,
        damage: 0,
        extra: None,
    })
}

fn logs(count: u32) -> Option<ItemStack> {
    stack("oak_log", count)
}

fn stage(carried: Option<ItemStack>, slots: Vec<Option<ItemStack>>) -> Stage {
    Stage { carried, slots }
}

// The inventory's slots: 0 the result, 1 to 4 the grid, 5 to 8 the armour (head first), 9 to 35
// the main inventory, 36 to 44 the hotbar, 45 the off hand.

#[test]
fn one_item_goes_on_the_target_and_the_rest_back() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(10));
        let plan = menu.plan_deal(env, 9, &[10], Split::One).unwrap();
        assert_eq!(plan.targets, [0]);
        assert_eq!(plan.picked, stage(logs(10), vec![None, None]));
        assert_eq!(plan.placed, stage(logs(9), vec![None, logs(1)]));
        assert_eq!(plan.returned, Some(stage(None, vec![logs(9), logs(1)])));
        assert_eq!(menu.slot(9), logs(10).as_ref(), "planned on a copy");
    });
}

#[test]
fn the_last_item_dealt_leaves_nothing_to_put_back() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(1));
        menu.set_slot(env, 10, logs(12));
        let plan = menu.plan_deal(env, 9, &[10], Split::One).unwrap();
        assert_eq!(plan.placed, stage(None, vec![None, logs(13)]));
        assert_eq!(plan.returned, None);
    });
}

#[test]
fn one_item_goes_nowhere_that_takes_no_exactly_one() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.creative = false;
        menu.set_slot(env, 9, logs(10));
        menu.set_slot(env, 10, logs(64));
        menu.set_slot(env, 11, stack("dirt", 3));
        let mut bundle = stack("bundle", 1).unwrap();
        bundle.data_mut().bundle_contents = vec![stack("dirt", 2).unwrap()];
        menu.set_slot(env, 12, Some(bundle));
        for (target, why) in [
            (10, "a full stack"),
            (11, "another item"),
            (12, "a bundle"),
            (0, "the result"),
            (5, "logs on the head"),
            (9, "the source itself"),
        ] {
            assert_eq!(menu.plan_deal(env, 9, &[target], Split::One), None, "{why}");
        }
        assert_eq!(
            menu.plan_deal(env, 9, &[10, 13], Split::One),
            None,
            "two targets"
        );
    });
}

#[test]
fn a_drag_shares_the_stack_evenly_and_puts_the_rest_back() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(10));
        let plan = menu.plan_deal(env, 9, &[10, 11, 12], Split::Even).unwrap();
        assert_eq!(plan.targets, [0, 1, 2]);
        assert_eq!(
            plan.placed,
            stage(logs(1), vec![None, logs(3), logs(3), logs(3)])
        );
        assert_eq!(
            plan.returned,
            Some(stage(None, vec![logs(1), logs(3), logs(3), logs(3)]))
        );
    });
}

#[test]
fn a_drag_tops_up_a_slot_to_its_stack_size() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(10));
        menu.set_slot(env, 10, logs(63));
        let plan = menu.plan_deal(env, 9, &[10, 11], Split::Even).unwrap();
        assert_eq!(
            plan.returned,
            Some(stage(None, vec![logs(4), logs(64), logs(5)]))
        );
    });
}

#[test]
fn a_drag_takes_slots_in_order_while_items_last() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(3));
        let plan = menu
            .plan_deal(env, 9, &[10, 11, 12, 13, 14], Split::Even)
            .unwrap();
        assert_eq!(plan.targets, [0, 1, 2]);
        assert_eq!(
            plan.placed,
            stage(None, vec![None, logs(1), logs(1), logs(1)])
        );
        assert_eq!(plan.returned, None);
    });
}

#[test]
fn a_drag_onto_one_slot_puts_the_whole_stack_there() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(10));
        menu.set_slot(env, 10, logs(60));
        let plan = menu.plan_deal(env, 9, &[10], Split::Even).unwrap();
        assert_eq!(plan.returned, Some(stage(None, vec![logs(6), logs(64)])));
    });
}

#[test]
fn a_drag_previews_what_it_places_and_what_goes_back() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(10));
        menu.set_slot(env, 10, logs(60));
        let preview = |targets: &[usize]| {
            let plan = menu.plan_deal(env, 9, targets, Split::Even).unwrap();
            plan.preview()
        };
        // As vanilla's drag over the same slots previews the stack in hand.
        let vanilla = |targets: &[usize]| {
            let mut held = menu.clone();
            held.click(env, 9, 0, Input::Pickup).unwrap();
            let (shown, left) = held.quick_craft_preview(env, 0, targets).unwrap();
            (shown.into_iter().map(Option::unwrap).collect(), left)
        };
        assert_eq!(preview(&[10, 11]), (vec![(64, true), (5, false)], 1));
        assert_eq!(preview(&[10, 11]), vanilla(&[10, 11]));
        assert_eq!(preview(&[11, 12, 13]), (vec![(3, false); 3], 1));
        assert_eq!(preview(&[11, 12, 13]), vanilla(&[11, 12, 13]));
        assert_eq!(preview(&[11]), (vec![(10, false)], 0), "the whole stack");
        assert_eq!(preview(&[10]), (vec![(64, true)], 6), "what fits");
    });
}

#[test]
fn a_drag_passes_over_the_source_revisits_and_slots_that_take_none() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.creative = false;
        menu.set_slot(env, 9, logs(10));
        menu.set_slot(env, 11, logs(64));
        menu.set_slot(env, 12, stack("dirt", 3));
        let path = [10, 9, 11, 12, 0, 10, 13];
        let plan = menu.plan_deal(env, 9, &path, Split::Even).unwrap();
        assert_eq!(plan.targets, [0, 6], "10 and 13");
        assert_eq!(plan.placed, stage(None, vec![None, logs(5), logs(5)]));
        assert_eq!(plan.returned, None);
        assert_eq!(menu.plan_deal(env, 9, &[11, 12, 0, 9], Split::Even), None);
    });
}

#[test]
fn a_deal_onto_the_grid_changes_its_result_and_nothing_else() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 9, logs(10));
        let plan = menu.plan_deal(env, 9, &[1], Split::One).unwrap();
        assert_eq!(plan.returned, Some(stage(None, vec![logs(9), logs(1)])));
    });
}

#[test]
fn a_deal_comes_from_stored_stacks_only() {
    with_menu(Kind::Inventory, |env, menu| {
        menu.set_slot(env, 1, logs(4));
        assert!(
            menu.plan_deal(env, 1, &[2], Split::One).is_some(),
            "a grid slot"
        );
        menu.set_slot(env, 36, logs(4));
        assert!(
            menu.plan_deal(env, 36, &[37], Split::One).is_some(),
            "the hotbar"
        );
        menu.set_slot(env, 5, stack("iron_helmet", 1));
        menu.set_slot(env, 45, stack("shield", 1));
        let mut bundle = stack("bundle", 1).unwrap();
        bundle.data_mut().bundle_contents = vec![stack("dirt", 2).unwrap()];
        menu.set_slot(env, 20, Some(bundle));
        menu.set_slot(env, 21, stack("bundle", 1));
        for (source, why) in [
            (5, "armour"),
            (45, "the off hand"),
            (20, "a filled bundle"),
            (21, "an empty bundle"),
            (0, "the result"),
            (30, "an empty slot"),
        ] {
            assert_eq!(
                menu.plan_deal(env, source, &[10], Split::One),
                None,
                "{why}"
            );
        }
        menu.carried = logs(2);
        assert_eq!(
            menu.plan_deal(env, 1, &[2], Split::One),
            None,
            "a stack in hand"
        );
    });
}

#[test]
fn a_container_that_only_stores_deals_and_a_station_input_does_not() {
    with_menu(Kind::Chest(3), |env, chest| {
        chest.set_slot(env, 0, logs(5));
        assert!(chest.plan_deal(env, 0, &[27], Split::One).is_some());
    });
    // The stonecutter's input picked up forgets the recipe chosen for it.
    with_menu(Kind::Stonecutter, |env, menu| {
        menu.set_slot(env, 0, stack("stone", 5));
        assert_eq!(menu.plan_deal(env, 0, &[2], Split::One), None);
        menu.set_slot(env, 2, stack("stone", 5));
        assert!(
            menu.plan_deal(env, 2, &[0], Split::One).is_some(),
            "from the inventory into the input"
        );
    });
    with_menu(Kind::Furnace("smelting"), |env, menu| {
        menu.set_slot(env, 0, stack("raw_iron", 5));
        assert_eq!(menu.plan_deal(env, 0, &[3], Split::One), None);
    });
}
