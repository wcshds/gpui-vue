//! Integration coverage for shared notifying refs.

use gpui_vue::{ChangeNotifier, reactive_ref, ref_};

#[test]
fn clones_share_the_same_value() {
    let count = ref_(1);
    let cloned = count.clone();
    let mut notifications = 0;

    assert!(count.ptr_eq(&cloned));
    assert!(cloned.set(2, &mut || notifications += 1));

    assert_eq!(count.get(), 2);
    assert_eq!(notifications, 1);
}

#[test]
fn read_does_not_require_the_value_to_be_clone() {
    #[derive(PartialEq)]
    struct NotClone(String);

    let value = reactive_ref(NotClone("vapor".to_owned()));

    assert_eq!(value.read(|value| value.0.len()), 5);
    assert!(value.read(|value| value.0.as_str() == "vapor"));
}

#[test]
fn assigning_an_equal_value_does_not_notify() {
    #[derive(Default)]
    struct Counter(usize);

    impl ChangeNotifier for Counter {
        fn notify(&mut self) {
            self.0 += 1;
        }
    }

    let title = ref_(String::from("GPUI"));
    let mut notifications = Counter::default();

    assert!(!title.set(String::from("GPUI"), &mut notifications));
    assert_eq!(notifications.0, 0);

    assert!(title.set(String::from("Vue"), &mut notifications));
    assert_eq!(title.get(), "Vue");
    assert_eq!(notifications.0, 1);
}

#[test]
fn update_notifies_only_when_the_final_value_changed() {
    let items = ref_(vec!["template"]);
    let mut notifications = 0;

    assert!(items.update(|items| items.push("style"), &mut || notifications += 1,));
    assert_eq!(items.get(), vec!["template", "style"]);
    assert_eq!(notifications, 1);

    assert!(!items.update(|_| {}, &mut || notifications += 1));
    assert_eq!(notifications, 1);

    // A mutation whose final state equals its initial state is also suppressed.
    assert!(!items.update(
        |items| {
            items.push("script");
            items.pop();
        },
        &mut || notifications += 1,
    ));
    assert_eq!(notifications, 1);
}

#[test]
fn notifier_can_read_the_value_after_mutation() {
    let count = ref_(0);
    let observed = count.clone();
    let mut rendered_values = Vec::new();

    assert!(count.set(1, &mut || rendered_values.push(observed.get())));
    assert_eq!(rendered_values, vec![1]);
}

#[test]
fn unit_is_a_silent_notifier() {
    let count = ref_(0);

    assert!(count.set(1, &mut ()));
    assert!(count.update(|count| *count += 1, &mut ()));
    assert_eq!(count.get(), 2);
}
