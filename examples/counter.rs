//! Desktop counter demonstrating the native GPUI template expansion.

use gpui_vue::gpui::{App, Application, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_vue::prelude::*;

component! {
    /// Mutable state and direct markup retained by the counter's GPUI entity.
    component Counter {
        state {
            /// Displayed count.
            count: Local<i32> = Local::new(0),
            /// Whether the derived details panel is rendered.
            show_details: Local<bool> = Local::new(true),
        }

        template(this, _window, cx) {
            <view class="w-full h-full flex flex-col items-center justify-center gap-6 p-8 bg-slate-950 text-white">
                <div class="flex flex-col items-center gap-2">
                    <text class="text-sm font-semibold text-blue-400">"GPUI · VAPOR-STYLE RUST"</text>
                    <text class="text-4xl font-bold">{format!("Count: {}", this.count.get())}</text>
                    <text class="text-sm text-slate-400">"No JavaScript runtime, VDOM, or CSS parser"</text>
                </div>

                <div class="flex flex-row gap-3">
                    <button
                        id="increment"
                        class="px-5 py-3 rounded-xl bg-blue-600 font-semibold hover:bg-blue-500 active:bg-blue-700"
                        @click={cx.listener(|this, _, _, cx| {
                            this.count.update(|count| count + 1, cx);
                        })}
                    >
                        "Increment"
                    </button>
                    <button
                        id="toggle-details"
                        class="px-5 py-3 rounded-xl border border-slate-600 font-semibold hover:bg-slate-800 active:bg-slate-700"
                        @click={cx.listener(|this, _, _, cx| {
                            this.show_details.update(|visible| !*visible, cx);
                        })}
                    >
                        "Toggle details"
                    </button>
                </div>

                <div
                    v-if={this.show_details.get()}
                    class="flex flex-col items-center gap-2 p-4 rounded-lg bg-slate-900 text-slate-300"
                >
                    <text>{format!("Double: {}", this.count.get() * 2)}</text>
                    <div class="flex flex-row gap-2">
                        <span
                            v-for={step in 1_usize..=3}
                            :key={("step", step)}
                            class="px-2 py-1 rounded bg-emerald-700 text-sm text-white"
                        >
                            {format!("+{step}")}
                        </span>
                    </div>
                </div>
            </view>
        }
    }
}

/// Opens the native desktop window and mounts the counter entity.
fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(720.0), px(520.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| Counter::new(CounterProps::new(), cx),
        )
        .expect("failed to open the counter window");
        cx.activate(true);
    });
}
