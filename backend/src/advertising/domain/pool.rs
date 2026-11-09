//! Ordered slots are identities, never weights. This module has no persistence dependencies.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub slot_id: Uuid,
    pub ad_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotInput {
    pub slot_id: Option<Uuid>,
    pub ad_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub expected_revision: i64,
    pub reason: String,
    pub slots: Vec<SlotInput>,
}
impl Publication {
    pub fn validate(&self) -> Result<()> {
        ensure(self.slots.len() <= 1000, "ad_pool_slot_limit")?;
        let mut ids = std::collections::HashSet::new();
        ensure(
            self.slots
                .iter()
                .filter_map(|s| s.slot_id)
                .all(|id| ids.insert(id)),
            "ad_pool_duplicate_slot",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reset {
    pub expected_revision: i64,
    pub expected_cursor_version: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Copy {
    pub target_route_id: String,
    pub expected_revision: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cursor {
    pub pool_revision: i64,
    pub cursor_version: i64,
    pub reset_epoch: i64,
    pub next_slot_id: Option<Uuid>,
    pub last_consumed_slot_id: Option<Uuid>,
    pub cycle: i64,
    pub selection_order: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scan {
    pub slot_id: Uuid,
    pub position: usize,
    pub ad_id: Uuid,
    pub exclusion: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selection {
    pub pool_id: Uuid,
    pub before: Cursor,
    pub after: Cursor,
    pub selected_slot_id: Option<Uuid>,
    pub selected_position: Option<usize>,
    pub receive_order: Option<i64>,
    pub rule_fingerprint: String,
    pub scans: Vec<Scan>,
}
pub fn scan_indices(slots: &[Slot], cursor: &Cursor) -> Vec<usize> {
    let start = slots
        .iter()
        .position(|s| Some(s.slot_id) == cursor.next_slot_id)
        .unwrap_or(0);
    (0..slots.len())
        .map(|i| (start + i) % slots.len())
        .collect()
}
pub fn advance(slots: &[Slot], before: &Cursor, selected: usize) -> Cursor {
    let start = slots
        .iter()
        .position(|s| Some(s.slot_id) == before.next_slot_id)
        .unwrap_or(0);
    let mut after = before.clone();
    after.cursor_version += 1;
    after.selection_order += 1;
    after.last_consumed_slot_id = Some(slots[selected].slot_id);
    after.next_slot_id = Some(slots[(selected + 1) % slots.len()].slot_id);
    after.cycle += i64::from(selected < start) + i64::from(selected + 1 == slots.len());
    after
}
pub fn published_cursor(slots: &[Slot], before: &Cursor) -> Cursor {
    let mut after = before.clone();
    after.pool_revision += 1;
    after.cursor_version += 1;
    after.cycle = 0;
    after.next_slot_id = slots
        .iter()
        .position(|s| Some(s.slot_id) == before.last_consumed_slot_id)
        .map(|i| slots[(i + 1) % slots.len()].slot_id)
        .or_else(|| slots.first().map(|s| s.slot_id));
    after
}
#[cfg(test)]
mod tests {
    use super::*;
    fn initial(slots: &[Slot]) -> Cursor {
        Cursor {
            pool_revision: 1,
            cursor_version: 0,
            reset_epoch: 0,
            next_slot_id: slots.first().map(|s| s.slot_id),
            last_consumed_slot_id: None,
            cycle: 0,
            selection_order: 0,
        }
    }
    #[test]
    fn slot_validation_allows_repeated_ads_but_not_ids_or_unbounded_sequences() {
        let ad = Uuid::new_v4();
        let slot = Uuid::new_v4();
        let mut input = Publication {
            expected_revision: 0,
            reason: "发布".into(),
            slots: vec![
                SlotInput {
                    slot_id: None,
                    ad_id: ad,
                },
                SlotInput {
                    slot_id: None,
                    ad_id: ad,
                },
            ],
        };
        input.validate().unwrap();
        input.slots.iter_mut().for_each(|s| s.slot_id = Some(slot));
        assert_eq!(input.validate().unwrap_err().0, "ad_pool_duplicate_slot");
        input.slots = vec![
            SlotInput {
                slot_id: None,
                ad_id: ad
            };
            1001
        ];
        assert_eq!(input.validate().unwrap_err().0, "ad_pool_slot_limit");
    }
    #[test]
    fn repeats_are_exact_and_edits_follow_slot_identity() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let slots: Vec<_> = [a, b, b, a]
            .into_iter()
            .map(|ad_id| Slot {
                slot_id: Uuid::new_v4(),
                ad_id,
            })
            .collect();
        let mut c = initial(&slots);
        let mut chosen = vec![];
        for _ in 0..8 {
            let i = scan_indices(&slots, &c)[0];
            chosen.push(slots[i].ad_id);
            c = advance(&slots, &c, i);
        }
        assert_eq!(chosen, vec![a, b, b, a, a, b, b, a]);
        assert_eq!(c.cycle, 2);
        assert_eq!(c.selection_order, 8);
        c = advance(&slots, &initial(&slots), 2);
        let moved = vec![
            slots[2].clone(),
            slots[0].clone(),
            slots[1].clone(),
            slots[3].clone(),
        ];
        assert_eq!(
            published_cursor(&moved, &c).next_slot_id,
            Some(slots[0].slot_id)
        );
        assert_eq!(
            published_cursor(&slots[..2], &c).next_slot_id,
            Some(slots[0].slot_id)
        );
    }
    #[test]
    fn bounded_scan_and_wrap() {
        let slots: Vec<_> = (0..4)
            .map(|_| Slot {
                slot_id: Uuid::new_v4(),
                ad_id: Uuid::new_v4(),
            })
            .collect();
        let mut c = initial(&slots);
        c.next_slot_id = Some(slots[1].slot_id);
        assert_eq!(scan_indices(&slots, &c), vec![1, 2, 3, 0]);
        assert_eq!(advance(&slots, &c, 3).next_slot_id, Some(slots[0].slot_id));
        assert_eq!(advance(&slots, &c, 0).cycle, 1);
        assert!(scan_indices(&[], &c).is_empty());
        assert_eq!(published_cursor(&[], &c).next_slot_id, None);
    }
}
