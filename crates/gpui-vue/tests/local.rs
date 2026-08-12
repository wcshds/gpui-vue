//! Integration tests for allocation-free component-local state and memoization.

use std::cell::Cell;

use gpui_vue::{Local, Memo, Revision};

#[test]
fn local_reads_inline_values_without_shared_storage() {
    let state = Local::new(String::from("vapor"));

    assert_eq!(state.get(), "vapor");
    assert_eq!(state.as_ref(), "vapor");
    assert_eq!(state.read(String::len), 5);
    assert_eq!(state.revision(), Revision::ZERO);
}

#[test]
fn set_only_advances_and_notifies_for_a_changed_value() {
    let mut state = Local::new(7_u32);
    let notifications = Cell::new(0_u32);
    let mut notifier = || notifications.set(notifications.get() + 1);

    assert!(!state.set(7, &mut notifier));
    assert_eq!(state.revision(), Revision::ZERO);
    assert_eq!(notifications.get(), 0);

    assert!(state.set(8, &mut notifier));
    assert_eq!(state.get(), 8);
    assert_eq!(state.revision(), Revision::from_raw(1));
    assert_eq!(notifications.get(), 1);
}

#[test]
fn update_compares_a_derived_value_without_cloning_the_previous_one() {
    #[derive(Debug, PartialEq)]
    struct NotClone(String);

    let mut state = Local::new(NotClone(String::from("GPUI")));
    let mut notifications = 0_u32;
    let mut notifier = || notifications += 1;

    assert!(!state.update(|current| NotClone(current.0.to_uppercase()), &mut notifier));
    assert!(state.update(|_| NotClone(String::from("GPUI Vue")), &mut notifier));

    assert_eq!(state.as_ref().0, "GPUI Vue");
    assert_eq!(notifications, 1);
}

#[test]
fn notification_observes_the_new_value_and_revision() {
    struct Observation<'a> {
        state: &'a Cell<Option<(u32, Revision)>>,
        next_value: u32,
        next_revision: Revision,
    }

    impl gpui_vue::ChangeNotifier for Observation<'_> {
        fn notify(&mut self) {
            self.state.set(Some((self.next_value, self.next_revision)));
        }
    }

    let mut state = Local::new(1_u32);
    let observed = Cell::new(None);
    let mut notifier = Observation {
        state: &observed,
        next_value: 2,
        next_revision: Revision::from_raw(1),
    };

    assert!(state.set(2, &mut notifier));
    assert_eq!(observed.get(), Some((2, Revision::from_raw(1))));
    assert_eq!(state.get(), 2);
    assert_eq!(state.revision(), Revision::from_raw(1));
}

#[test]
fn revision_wraps_without_overflowing() {
    assert_eq!(Revision::MAX.next(), Revision::ZERO);
    assert_ne!(Revision::MAX, Revision::MAX.next());
}

#[test]
fn memo_reuses_results_until_its_revision_changes() {
    let mut state = Local::new(3_u32);
    let mut memo = Memo::<u32>::new();
    let computations = Cell::new(0_u32);

    let first = *memo.get_or_update(state.revision(), || {
        computations.set(computations.get() + 1);
        state.as_ref() * 2
    });
    let cached = *memo.get_or_update(state.revision(), || {
        computations.set(computations.get() + 1);
        99
    });

    assert_eq!((first, cached), (6, 6));
    assert_eq!(computations.get(), 1);

    assert!(state.set(4, &mut ()));
    let refreshed = *memo.get_or_update(state.revision(), || {
        computations.set(computations.get() + 1);
        state.as_ref() * 2
    });

    assert_eq!(refreshed, 8);
    assert_eq!(computations.get(), 2);
}

#[test]
fn memo_supports_typed_multi_state_dependencies_and_invalidation() {
    let left = Local::new(2_u32);
    let right = Local::new(5_u32);
    let mut memo = Memo::<u32, (Revision, Revision)>::new();

    assert_eq!(
        *memo.get_or_update((left.revision(), right.revision()), || {
            left.as_ref() + right.as_ref()
        }),
        7
    );
    assert_eq!(memo.dependencies(), Some(&(Revision::ZERO, Revision::ZERO)));

    memo.invalidate();
    assert!(memo.get().is_none());
    assert!(memo.dependencies().is_none());
}

#[test]
fn local_supports_default_from_and_into_inner() {
    let defaulted = Local::<Vec<u8>>::default();
    let converted = Local::from(String::from("owned"));

    assert!(defaulted.as_ref().is_empty());
    assert_eq!(converted.into_inner(), "owned");
}
