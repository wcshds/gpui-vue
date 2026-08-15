//! Virtualized native lists for large data sets.
//!
//! Use [`uniform_list`] when every row has the same measured height. Use
//! [`list`] with a retained [`ListState`] when rows have independent heights.

pub use gpui::{
    List, ListAlignment, ListHorizontalSizingBehavior, ListMeasuringBehavior, ListOffset,
    ListScrollEvent, ListSizingBehavior, ListState, ScrollStrategy, UniformList,
    UniformListScrollHandle, list, uniform_list,
};
