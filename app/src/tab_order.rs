use crate::groups::TabItem;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabOrder {
    items: Vec<(TabItem, bool)>,
}

impl TabOrder {
    pub fn reconcile(&mut self, present: &[TabItem]) {
        self.items.retain(|(item, _)| present.contains(item));
        for item in present {
            if !self.contains(*item) {
                self.items.push((*item, false));
            }
        }
    }

    pub fn reconciled(&self, present: &[TabItem]) -> TabOrder {
        let mut order = self.clone();
        order.reconcile(present);
        order
    }

    pub fn items(&self) -> impl Iterator<Item = TabItem> + '_ {
        self.items.iter().map(|(item, _)| *item)
    }

    pub fn pinned_count(&self) -> usize {
        self.items.iter().take_while(|(_, pinned)| *pinned).count()
    }

    pub fn contains(&self, item: TabItem) -> bool {
        self.position(item).is_some()
    }

    pub fn position(&self, item: TabItem) -> Option<usize> {
        self.items.iter().position(|(i, _)| *i == item)
    }

    pub fn is_pinned(&self, item: TabItem) -> bool {
        self.items.iter().any(|(i, pinned)| *i == item && *pinned)
    }

    pub fn pin(&mut self, item: TabItem) {
        if self.is_pinned(item) {
            return;
        }
        self.remove(item);
        let at = self.pinned_count();
        self.items.insert(at, (item, true));
    }

    pub fn unpin(&mut self, item: TabItem) {
        if !self.is_pinned(item) {
            return;
        }
        self.remove(item);
        let at = self.pinned_count();
        self.items.insert(at, (item, false));
    }

    pub fn remove(&mut self, item: TabItem) {
        self.items.retain(|(i, _)| *i != item);
    }

    pub fn insert_after(&mut self, anchor: Option<TabItem>, item: TabItem) {
        self.remove(item);
        let at = match anchor.and_then(|anchor| self.position(anchor)) {
            Some(at) => (at + 1).max(self.pinned_count()),
            None => self.items.len(),
        };
        self.items.insert(at, (item, false));
    }

    pub fn insert_pinned(&mut self, item: TabItem) {
        self.remove(item);
        let at = self.pinned_count();
        self.items.insert(at, (item, true));
    }

    pub fn replace(&mut self, old: TabItem, new: TabItem) {
        if let Some(entry) = self.items.iter_mut().find(|(i, _)| *i == old) {
            entry.0 = new;
        }
    }

    pub fn reorder(&mut self, item: TabItem, before: TabItem) -> bool {
        let (Some(from), Some(to)) = (self.position(item), self.position(before)) else {
            return false;
        };
        if from == to || self.items[from].1 != self.items[to].1 {
            return false;
        }
        let entry = self.items.remove(from);
        let to = if from < to { to - 1 } else { to };
        let at = if from <= to { to + 1 } else { to };
        self.items.insert(at, entry);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::Pane;

    fn order(items: &[TabItem]) -> TabOrder {
        let mut order = TabOrder::default();
        order.reconcile(items);
        order
    }

    fn files(order: &TabOrder) -> Vec<u64> {
        order
            .items()
            .map(|item| match item {
                TabItem::Pane(Pane::File(n)) => n,
                _ => 0,
            })
            .collect()
    }

    fn tab(n: u64) -> TabItem {
        TabItem::Pane(Pane::File(n))
    }

    #[test]
    fn pinned_tabs_come_first_in_the_order_they_were_pinned() {
        let mut order = order(&[tab(1), tab(2), tab(3), tab(4)]);
        order.pin(tab(3));
        order.pin(tab(1));
        assert_eq!(files(&order), [3, 1, 2, 4]);
        assert_eq!(order.pinned_count(), 2);
        order.pin(tab(3));
        assert_eq!(files(&order), [3, 1, 2, 4], "pinning twice changes nothing");
    }

    #[test]
    fn unpinning_moves_the_tab_to_the_first_place_after_the_pinned_ones() {
        let mut order = order(&[tab(1), tab(2), tab(3), tab(4)]);
        order.pin(tab(4));
        order.pin(tab(2));
        order.unpin(tab(4));
        assert_eq!(files(&order), [2, 4, 1, 3]);
        assert!(!order.is_pinned(tab(4)));
        assert_eq!(order.pinned_count(), 1);
    }

    #[test]
    fn a_new_tab_never_lands_among_the_pinned_ones() {
        let mut order = order(&[tab(1), tab(2), tab(3)]);
        order.pin(tab(1));
        order.pin(tab(2));
        order.insert_after(Some(tab(1)), tab(9));
        assert_eq!(files(&order), [1, 2, 9, 3]);
        order.insert_after(Some(tab(3)), tab(8));
        assert_eq!(files(&order), [1, 2, 9, 3, 8]);
        order.insert_after(None, tab(7));
        assert_eq!(files(&order), [1, 2, 9, 3, 8, 7]);
        order.insert_pinned(tab(6));
        assert_eq!(files(&order), [1, 2, 6, 9, 3, 8, 7]);
        assert!(order.is_pinned(tab(6)));
    }

    #[test]
    fn a_drag_reorders_inside_its_side_of_the_pins_only() {
        let mut order = order(&[tab(1), tab(2), tab(3), tab(4), tab(5)]);
        order.pin(tab(1));
        order.pin(tab(2));
        assert!(order.reorder(tab(2), tab(1)));
        assert_eq!(files(&order), [2, 1, 3, 4, 5]);
        assert!(order.reorder(tab(3), tab(5)));
        assert_eq!(files(&order), [2, 1, 4, 5, 3]);
        assert!(order.reorder(tab(3), tab(4)));
        assert_eq!(files(&order), [2, 1, 3, 4, 5]);
        assert!(!order.reorder(tab(4), tab(1)), "across the pins");
        assert!(!order.reorder(tab(1), tab(4)), "across the pins");
        assert_eq!(files(&order), [2, 1, 3, 4, 5]);
    }

    #[test]
    fn closed_tabs_leave_and_new_ones_come_last_unpinned() {
        let mut order = order(&[tab(1), tab(2), tab(3)]);
        order.pin(tab(2));
        order.reconcile(&[tab(1), tab(3), tab(4)]);
        assert_eq!(files(&order), [1, 3, 4]);
        assert_eq!(order.pinned_count(), 0);
        order.pin(tab(3));
        order.replace(tab(1), tab(5));
        assert_eq!(files(&order), [3, 5, 4]);
    }
}
