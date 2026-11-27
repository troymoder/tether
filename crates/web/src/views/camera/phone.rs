use android::scrcpy;
use leptos::prelude::*;
use protocol::CameraMode;

use super::mirror::Mirror;
use super::mode::{key, mode_text, shown as shown_at};
use super::pick::Pick;
use super::{Camera, said, style};
use crate::app::Picked;
use crate::ui;
use crate::ui::kept::kept_signal;
use crate::ui::load::use_load;
use crate::ui::loadable::Status;
use crate::ui::pane::Pane;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Phone() -> impl IntoView {
    let device = expect_context::<Picked>().device;
    let cameras = use_load(
        || (),
        move |()| {
            let device = device.get_untracked();

            async move { Camera::cameras(&*device).await }
        },
    );
    let picked = RwSignal::new(None::<String>);
    // Held here, since the control is built again for each camera.
    let open = RwSignal::new(false);

    view! {
        <Status of=cameras loading="Looking for cameras…" line=|what: String| said(what).into_any() let:cameras>
            {
                let picked_id = picked.get();
                let camera = cameras
                    .iter()
                    .find(|camera| Some(&camera.id) == picked_id.as_ref())
                    .or_else(|| cameras.first())
                    .cloned();

                match camera {
                    Some(camera) => view! { <OneCamera camera cameras picked open /> }.into_any(),
                    None => said("No cameras".to_owned()).into_any(),
                }
            }
        </Status>
    }
}

#[component]
fn OneCamera(
    camera: scrcpy::Camera,
    cameras: Vec<scrcpy::Camera>,
    picked: RwSignal<Option<String>>,
    open: RwSignal<bool>,
) -> impl IntoView {
    let device = expect_context::<Picked>().id.to_string();
    let kept = kept_signal(key("mode", &device, &camera), None::<String>);
    let camera = StoredValue::new(camera);
    // None until the browser's database has answered, so the camera is not
    // opened at the usual mode first.
    let shown = Memo::new(move |_| {
        kept.get()
            .map(|kept| shown_at(camera.get_value(), kept.as_deref()))
    });

    view! {
        <Pane
            title="Camera"
            class=style::view
            tools=move || {
                view! {
                    <Pick
                        cameras=cameras.clone()
                        shown
                        on_camera=move |id: String| picked.set(Some(id))
                        on_mode=move |mode: CameraMode| kept.set(Some(mode_text(&mode)))
                        open
                    />
                }
            }
        >
            <div class=ui::style::pane_body>
                {move || shown.get().map(|shown| view! { <Mirror shown /> })}
            </div>
        </Pane>
    }
}
