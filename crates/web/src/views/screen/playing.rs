//! The screen's state in one value, and the steps from one state to the next.
//! Nothing here touches the browser, so the steps are tested directly.
//! stream.rs takes the steps and holds what keeps a stream running.

use protocol::Status;

use super::Turned;

/// The number of retries after streams end in a row, one, two and four
/// seconds apart, before the screen is given up on.
pub(super) const TRIES: u32 = 3;

/// A picture's width and height in pixels.
pub(crate) type Size = (u32, u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Wanted {
    Nothing,
    Pictures,
    Stream,
}

impl Wanted {
    pub(super) fn of(hidden: bool, ready: bool, status: Option<Status>) -> Self {
        // A booting emulator has no video yet and a stopping one has none
        // left, so tetherd's pictures stand in meanwhile.
        let pictures = match status {
            Some(Status::Stopping) => true,
            Some(Status::Booting) => !ready,
            _ => false,
        };
        // An emulator that just shut down is still listed as ready for a
        // moment and must not be streamed then.
        let streams = ready && status != Some(Status::Shutdown);

        if hidden {
            Wanted::Nothing
        } else if pictures {
            Wanted::Pictures
        } else if streams {
            Wanted::Stream
        } else {
            Wanted::Nothing
        }
    }
}

/// `drawn` is the size of the picture still shown from before, under any note
/// over it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Playing {
    Idle,
    Pictures {
        drawn: Option<Size>,
    },
    /// `tries` counts the streams that ended in a row before this one.
    /// `turned` is set if the device reported its rotation before the first
    /// frame.
    Starting {
        tries: u32,
        drawn: Option<Size>,
        turned: Option<Turned>,
    },

    Live {
        size: Size,
        turned: Option<Turned>,
    },
    Retrying {
        tries: u32,
        drawn: Option<Size>,
    },
    /// Only Reconnect or a change of the device requests the stream again.
    GaveUp {
        drawn: Option<Size>,
    },
}

impl Playing {
    pub(crate) fn live(&self) -> bool {
        matches!(self, Playing::Live { .. })
    }

    pub(super) fn drawn(&self) -> Option<Size> {
        match *self {
            Playing::Idle => None,
            Playing::Live { size, .. } => Some(size),
            Playing::Pictures { drawn }
            | Playing::Starting { drawn, .. }
            | Playing::Retrying { drawn, .. }
            | Playing::GaveUp { drawn } => drawn,
        }
    }

    pub(super) fn turned(&self) -> Option<Turned> {
        match *self {
            Playing::Live { turned, .. } => turned,
            _ => None,
        }
    }

    /// The seconds to wait before the stream is requested again: one, two,
    /// then four.
    pub(super) fn wait(&self) -> Option<u64> {
        match *self {
            Playing::Retrying { tries, .. } => Some(1 << tries.saturating_sub(1)),
            _ => None,
        }
    }

    /// `moved` is true if the device or the pane changed since the last
    /// request, which resets the retry count.
    pub(super) fn asked(self, wanted: Wanted, moved: bool) -> Self {
        let drawn = self.drawn();
        let tries = match self {
            _ if moved => 0,
            Playing::Starting { tries, .. } | Playing::Retrying { tries, .. } => tries,
            // The connection came back after the stream was given up on,
            // which earns one more try.
            Playing::GaveUp { .. } => TRIES,
            Playing::Idle | Playing::Pictures { .. } | Playing::Live { .. } => 0,
        };

        match wanted {
            Wanted::Nothing => Playing::Idle,
            Wanted::Pictures => Playing::Pictures { drawn },
            Wanted::Stream => Playing::Starting {
                tries,
                drawn,
                turned: None,
            },
        }
    }

    pub(super) fn pictured(self, size: Size) -> Self {
        match self {
            Playing::Pictures { .. } => Playing::Pictures { drawn: Some(size) },
            other => other,
        }
    }

    pub(super) fn framed(self, size: Size) -> Self {
        match self {
            Playing::Starting { turned, .. } | Playing::Live { turned, .. } => {
                Playing::Live { size, turned }
            }
            other => other,
        }
    }

    pub(super) fn read(self, turned: Turned) -> Self {
        let turned = Some(turned);

        match self {
            Playing::Starting { tries, drawn, .. } => Playing::Starting {
                tries,
                drawn,
                turned,
            },
            Playing::Live { size, .. } => Playing::Live { size, turned },
            other => other,
        }
    }

    pub(super) fn ended(self) -> Self {
        let (tries, drawn) = match self {
            Playing::Starting { tries, drawn, .. } => (tries + 1, drawn),
            Playing::Live { size, .. } => (1, Some(size)),
            other => return other,
        };

        if tries > TRIES {
            Playing::GaveUp { drawn }
        } else {
            Playing::Retrying { tries, drawn }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = (1080, 2400);

    fn starting(tries: u32, drawn: Option<Size>) -> Playing {
        Playing::Starting {
            tries,
            drawn,
            turned: None,
        }
    }

    #[test]
    fn the_device_and_the_pane_say_what_is_asked() {
        assert_eq!(Wanted::of(false, true, None), Wanted::Stream);
        assert_eq!(Wanted::of(true, true, None), Wanted::Nothing);
        assert_eq!(Wanted::of(false, false, None), Wanted::Nothing);
        assert_eq!(
            Wanted::of(false, false, Some(Status::Booting)),
            Wanted::Pictures
        );
        assert_eq!(
            Wanted::of(false, true, Some(Status::Booting)),
            Wanted::Stream
        );
        assert_eq!(
            Wanted::of(false, true, Some(Status::Stopping)),
            Wanted::Pictures
        );
        assert_eq!(
            Wanted::of(false, true, Some(Status::Shutdown)),
            Wanted::Nothing
        );
        assert_eq!(
            Wanted::of(false, true, Some(Status::Booted)),
            Wanted::Stream
        );
    }

    #[test]
    fn a_booting_emulator_goes_from_pictures_to_video() {
        let booting = Playing::Idle.asked(Wanted::Pictures, true);

        assert_eq!(booting, Playing::Pictures { drawn: None });

        let pictured = booting.pictured(SIZE);

        assert_eq!(pictured.drawn(), Some(SIZE));

        let booted = pictured.asked(Wanted::Stream, true);

        assert_eq!(booted, starting(0, Some(SIZE)));
        assert!(!booted.live());

        let live = booted.framed((2400, 1080));

        assert_eq!(
            live,
            Playing::Live {
                size: (2400, 1080),
                turned: None
            }
        );
        assert!(live.live());
    }

    #[test]
    fn a_turn_read_before_the_first_frame_is_kept() {
        let turned = Turned {
            degrees: 90,
            auto: Some(true),
        };
        let live = starting(0, None).read(turned).framed(SIZE);

        assert_eq!(live.turned(), Some(turned));
        assert_eq!(starting(0, None).read(turned).turned(), None);
        assert_eq!(Playing::Idle.read(turned), Playing::Idle);
    }

    #[test]
    fn a_stream_that_ends_is_retried_then_given_up_on() {
        let live = starting(0, None).framed(SIZE);
        let mut now = live.ended();

        assert_eq!(
            now,
            Playing::Retrying {
                tries: 1,
                drawn: Some(SIZE)
            }
        );

        for (tries, wait) in [(1, 1), (2, 2), (3, 4)] {
            assert_eq!(now.wait(), Some(wait));

            now = now.asked(Wanted::Stream, false);

            assert_eq!(now, starting(tries, Some(SIZE)));

            now = now.ended();
        }

        assert_eq!(now, Playing::GaveUp { drawn: Some(SIZE) });
        assert_eq!(now.wait(), None);
        assert_eq!(now.ended(), now);
        assert_eq!(Playing::Idle.ended(), Playing::Idle);
    }

    #[test]
    fn a_frame_forgets_the_streams_that_ended() {
        let again = starting(2, Some(SIZE)).framed(SIZE).ended();

        assert_eq!(
            again,
            Playing::Retrying {
                tries: 1,
                drawn: Some(SIZE)
            }
        );
    }

    #[test]
    fn a_change_of_the_device_starts_over() {
        let gave_up = Playing::GaveUp { drawn: Some(SIZE) };

        assert_eq!(gave_up.asked(Wanted::Stream, true), starting(0, Some(SIZE)));
        assert_eq!(
            gave_up.asked(Wanted::Stream, false),
            starting(TRIES, Some(SIZE))
        );
        assert_eq!(gave_up.asked(Wanted::Nothing, true), Playing::Idle);
        assert_eq!(
            Playing::Retrying {
                tries: 2,
                drawn: None
            }
            .asked(Wanted::Stream, true),
            starting(0, None)
        );
    }

    #[test]
    fn a_stale_picture_or_frame_changes_nothing() {
        assert_eq!(Playing::Idle.pictured(SIZE), Playing::Idle);
        assert_eq!(Playing::Idle.framed(SIZE), Playing::Idle);
        assert_eq!(
            Playing::GaveUp { drawn: None }.framed(SIZE),
            Playing::GaveUp { drawn: None }
        );
    }
}
