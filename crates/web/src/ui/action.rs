use std::future::Future;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::{Global, Picked};

/// A run while it is busy is dropped, so a second press does nothing. `what`
/// names the action for tetherd's events when the page does it by itself, and
/// is None for a call to tetherd, which tetherd logs by itself.
pub(crate) fn use_action<I, Said, Fut>(
    what: Option<&'static str>,
    act: impl Fn(I) -> Fut + 'static,
) -> (Signal<bool>, Callback<I>)
where
    I: 'static,
    Said: Into<Option<String>>,
    Fut: Future<Output = Result<Said, String>> + 'static,
{
    action(what, act, Meanwhile::Dropped)
}

pub(crate) fn use_action_many<I, Said, Fut>(
    what: Option<&'static str>,
    act: impl Fn(I) -> Fut + 'static,
) -> (Signal<bool>, Callback<I>)
where
    I: 'static,
    Said: Into<Option<String>>,
    Fut: Future<Output = Result<Said, String>> + 'static,
{
    action(what, act, Meanwhile::Beside)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Meanwhile {
    Dropped,
    Beside,
}

fn action<I, Said, Fut>(
    what: Option<&'static str>,
    act: impl Fn(I) -> Fut + 'static,
    meanwhile: Meanwhile,
) -> (Signal<bool>, Callback<I>)
where
    I: 'static,
    Said: Into<Option<String>>,
    Fut: Future<Output = Result<Said, String>> + 'static,
{
    let link = expect_context::<Global>().link;
    // The device of the view this belongs to, which its events are filed
    // under even if another is picked by the time a run ends.
    let about = use_context::<Picked>().map(|picked| picked.id);
    let running = RwSignal::new(0_usize);
    let busy = Memo::new(move |_| running.get() > 0);
    let act = StoredValue::new_local(act);
    let run = Callback::new(move |input: I| {
        if meanwhile == Meanwhile::Dropped && running.get_untracked() > 0 {
            return;
        }

        let Some(doing) = act.try_with_value(|act| act(input)) else {
            return;
        };

        let about = about.clone();

        running.update(|running| *running += 1);
        spawn_local(async move {
            let done = doing.await;

            // The component may be gone by now. The user is told all the
            // same.
            running.try_update(|running| *running -= 1);

            let told = match done.map(Into::into) {
                Ok(Some(said)) => Ok(said),
                Ok(None) => return,
                Err(why) => Err(why),
            };

            match (what, told) {
                (Some(what), told) => link.did(what, about.as_ref(), told),
                (None, Ok(said) | Err(said)) => link.say(said),
            }
        });
    });

    (busy.into(), run)
}
