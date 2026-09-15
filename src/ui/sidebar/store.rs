//! Reconciling the sidebar's `gio::ListStore` with a freshly built row list.
//!
//! Split out of `sidebar.rs`. Kept together because they are one concern: the
//! store is diffed in place rather than rebuilt, so rows keep their expanded
//! state and the selection survives a refresh.

use super::*;

/// Maps each row's identity to the widget currently in the store, so a rebuild
/// can reuse the same widgets (and therefore the scroll anchor) instead of
/// recreating them.
///
/// Invariant: the identities are unique within a list, which holds because the
/// API returns at most one repository per id. If a repository id ever appeared
/// twice, `target` would hold the same widget twice and `sync_store` would
/// parent it twice — a GTK-critical with no fallback — so the uniqueness is
/// load-bearing, not incidental.
pub(crate) fn snapshot_rows(store: &gio::ListStore) -> HashMap<RowIdentity, gtk::Widget> {
    let mut existing = HashMap::new();
    for i in 0..store.n_items() {
        let Some(item) = store.item(i) else {
            continue;
        };
        let Some(row) = item.downcast_ref::<gtk::Widget>() else {
            continue;
        };
        let Some(identity) = read_row_identity(row) else {
            continue;
        };
        existing.insert(identity, row.clone());
    }
    existing
}

/// Replaces `store`'s contents with `target` with a minimal diff, so rows that
/// survive keep their widget identity and stay parented.
///
/// `GtkListBase`'s scroll anchor is a tracker bound to a row's widget; its
/// item manager re-positions the tracker across model changes while a widget
/// survives, so the list holds its place without any restore. Rows absent from
/// `target` (stale headers, filtered-out repos) are removed and rows absent
/// from the store are inserted; rows present in both are never touched.
pub(crate) fn sync_store(store: &gio::ListStore, target: &[gtk::Widget]) {
    let n = store.n_items() as usize;
    let m = target.len();

    // Fast path for the common case — a no-op rebuild (nothing changed) leaves
    // the model identical, so a pointer walk decides it in O(n) with no
    // allocation. The LCS below is only paid when something actually moved.
    if n == m
        && (0..n).all(|i| {
            let Some(obj) = store.item(i as u32) else {
                return false;
            };
            (obj.as_ptr() as *const ())
                == (target[i].upcast_ref::<glib::Object>().as_ptr() as *const ())
        })
    {
        return;
    }

    let current: Vec<*const ()> = (0..n)
        .filter_map(|i| store.item(i as u32))
        .map(|item| item.as_ptr() as *const ())
        .collect();
    let desired: Vec<*const ()> = target
        .iter()
        .map(|row| row.upcast_ref::<glib::Object>().as_ptr() as *const ())
        .collect();

    // Longest common subsequence by widget identity.
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if current[i] == desired[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut matched_store = vec![false; n];
    let mut matched_target = vec![false; m];
    let mut i = 0usize;
    let mut j = 0usize;
    while i < n && j < m {
        if current[i] == desired[j] {
            matched_store[i] = true;
            matched_target[j] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    // Remove the rows that are not part of the common subsequence, back to
    // front so the indices stay valid.
    for i in (0..n).rev() {
        if !matched_store[i] {
            store.remove(i as u32);
        }
    }

    // Insert the missing rows in order. After the removals the matched rows
    // already sit at their target indices minus the inserts before them, so
    // position k of `target` is where `target[k]` belongs.
    for (k, row) in target.iter().enumerate() {
        if !matched_target[k] {
            store.insert(k as u32, row);
        }
    }
}

pub(crate) fn find_repo_index(model: &gtk::FilterListModel, repo_id: i64) -> Option<u32> {
    for idx in 0..model.n_items() {
        if let Some(obj) = model.item(idx)
            && let Some(id) = repo_id_from_object(&obj)
            && id == repo_id
        {
            return Some(idx);
        }
    }
    None
}

pub(crate) fn find_first_repo_index(model: &gtk::FilterListModel) -> Option<u32> {
    for idx in 0..model.n_items() {
        if let Some(obj) = model.item(idx)
            && repo_id_from_object(&obj).is_some()
        {
            return Some(idx);
        }
    }
    None
}
