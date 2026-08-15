//! Declarative controls shared by the KAGE Editor package.

use gpui_vue::prelude::*;
use gpui_vue::ui::{App, ClickEvent, ElementId, IntoElement, SharedString, Window};

/// Builds a compact toolbar button with macOS-style restraint.
pub(crate) fn toolbar_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    enabled: bool,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id = id.into();
    let label = label.into();
    if enabled {
        view! {
            <div
                :id={id}
                class="h-[30px] min-w-[30px] px-2 flex items-center justify-center rounded-md text-[13px] text-[#e2e2e6] bg-[#2a2a2e] border border-[#38383d] cursor-pointer hover:bg-[#3a3a3f] active:bg-[#202024]"
                :class={if active {
                    "bg-[#0a84ff] border-[#2792ff] text-white"
                } else {
                    "bg-[#2a2a2e]"
                }}
                @click={handler}
            >
                {label}
            </div>
        }
    } else {
        view! {
            <div
                :id={id}
                class="h-[30px] min-w-[30px] px-2 flex items-center justify-center rounded-md text-[13px] text-[#e2e2e6] bg-[#2a2a2e] border border-[#38383d] opacity-[0.34]"
                :class={if active {
                    "bg-[#0a84ff] border-[#2792ff] text-white"
                } else {
                    "bg-[#2a2a2e]"
                }}
            >
                {label}
            </div>
        }
    }
}

/// Builds a fixed-size toolbar icon button with a larger, optically clear glyph.
pub(crate) fn toolbar_icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<SharedString>,
    active: bool,
    enabled: bool,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id = id.into();
    let icon = icon.into();
    if enabled {
        view! {
            <div
                :id={id}
                class="w-[30px] h-[30px] flex-none flex items-center justify-center rounded-md text-[19px] leading-[20px] text-[#e2e2e6] bg-[#2a2a2e] border border-[#38383d] cursor-pointer hover:bg-[#3a3a3f] active:bg-[#202024]"
                :class={if active {
                    "bg-[#0a84ff] border-[#2792ff] text-white"
                } else {
                    "bg-[#2a2a2e]"
                }}
                @click={handler}
            >
                {icon}
            </div>
        }
    } else {
        view! {
            <div
                :id={id}
                class="w-[30px] h-[30px] flex-none flex items-center justify-center rounded-md text-[19px] leading-[20px] text-[#e2e2e6] bg-[#2a2a2e] border border-[#38383d] opacity-[0.34]"
                :class={if active {
                    "bg-[#0a84ff] border-[#2792ff] text-white"
                } else {
                    "bg-[#2a2a2e]"
                }}
            >
                {icon}
            </div>
        }
    }
}

/// Builds a square tool-rail button with a symbol above a tiny caption.
pub(crate) fn tool_button(
    id: impl Into<ElementId>,
    symbol: impl Into<SharedString>,
    caption: impl Into<SharedString>,
    active: bool,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id = id.into();
    let symbol = symbol.into();
    let caption = caption.into();
    view! {
        <div
            :id={id}
            class="w-[42px] h-[42px] flex flex-col items-center justify-center gap-[2px] rounded-md cursor-pointer text-[#adadb4] hover:bg-[#2a2a2e] hover:text-[#f4f4f6] active:bg-[#202024]"
            :class={if active {
                "bg-[#303036] text-[#73b7ff]"
            } else {
                "bg-transparent"
            }}
            @click={handler}
        >
            <div class="text-[20px] leading-[20px]">{symbol}</div>
            <div class="text-[10px] leading-[12px]">{caption}</div>
        </div>
    }
}

/// Builds a sidebar section title and optional trailing value.
pub(crate) fn section_header(
    title: impl Into<SharedString>,
    trailing: impl Into<SharedString>,
) -> impl IntoElement {
    let title = title.into();
    let trailing = trailing.into();
    view! {
        <div class="h-[34px] flex items-center justify-between text-[12px] font-semibold text-[#a0a0a7]">
            {title}
            <div class="font-normal text-[#85858c]">{trailing}</div>
        </div>
    }
}

/// Builds a dense label/value row used by the inspector.
pub(crate) fn inspector_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
) -> impl IntoElement {
    let label = label.into();
    let value = value.into();
    view! {
        <div class="min-h-[32px] flex items-center justify-between gap-3 text-[13px]">
            <div class="text-[#a0a0a7]">{label}</div>
            <div class="text-[#e5e5e8] text-ellipsis">{value}</div>
        </div>
    }
}

/// Builds a hairline divider for toolbars and inspectors.
pub(crate) fn separator(vertical: bool) -> impl IntoElement {
    view! {
        <div
            class="bg-[#3a3a3e]"
            :class={if vertical {
                "w-px h-[18px] mx-1"
            } else {
                "h-px w-full my-1"
            }}
        />
    }
}
