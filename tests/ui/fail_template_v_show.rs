//! A structural template has no rendered host on which to apply `v-show`.

use gpui_vue::view;

fn main() {
    let visible = true;
    let _ = view! {
        <div>
            <template v-show={visible}>"hidden"</template>
        </div>
    };
}
