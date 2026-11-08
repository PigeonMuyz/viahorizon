// SPDX-License-Identifier: GPL-3.0-or-later
// Execute the packaged helper with a small boundary fixture. The real game
// signature and field access are also checked by the pinned native build.
extern crate self as horizon_client;
extern crate self as horizon_core;

type Result<T> = std::result::Result<T, &'static str>;

pub mod geometry {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Vec3(pub f64, pub f64, pub f64);
    impl Vec3 {
        pub const ZERO: Self = Self(0.0, 0.0, 0.0);
    }
}
pub mod net {
    pub mod vfp_capture_session {
        pub fn legacy_protocol(session: &Option<i32>) -> Option<i32> {
            *session
        }
    }
}
mod game {
    use crate::geometry::Vec3;
    pub struct Remote {
        pub session: Option<i32>,
    }
    pub struct Boat {
        pub id: u32,
    }
    pub struct Player {
        pub position: Vec3,
        pub velocity: Vec3,
        pub fall_distance: f32,
    }
    pub struct Game<'a> {
        pub remote: Option<Remote>,
        pub boats: Vec<Boat>,
        pub riding: Option<u32>,
        pub player: Option<Player>,
        pub previous_position: Vec3,
        pub seat: &'a Vec3,
    }
    impl Game<'_> {
        pub fn player_boat_seat(&self, _: usize) -> Vec3 {
            *self.seat
        }
    }
}
mod helper {
    use crate::Result;
    include!("../mixins/viahorizon/legacy_boat_ride.rs");

    #[test]
    fn legacy_riders_return_a_successful_result_and_follow_the_server_seat() {
        use crate::{game::*, geometry::Vec3};
        for protocol in [4, 5, 47] {
            let seat = Vec3(10.0, 64.5, 20.0);
            let mut game = Game {
                remote: Some(Remote {
                    session: Some(protocol),
                }),
                boats: vec![Boat { id: 42 }],
                riding: Some(42),
                player: Some(Player {
                    position: Vec3(8.0, 63.0, 20.0),
                    velocity: Vec3(1.0, -2.0, 3.0),
                    fall_distance: 4.0,
                }),
                previous_position: Vec3::ZERO,
                seat: &seat,
            };
            let result: core::ops::ControlFlow<Result<bool>> = inject(&mut game);
            assert_eq!(result, core::ops::ControlFlow::Break(Ok(true)));
            assert_eq!(game.previous_position, Vec3(8.0, 63.0, 20.0));
            let player = game.player.unwrap();
            assert_eq!(player.position, seat);
            assert_eq!(player.velocity, Vec3::ZERO);
            assert_eq!(player.fall_distance, 0.0);
        }
    }

    #[test]
    fn modern_connections_and_missing_mounts_continue_normal_gameplay() {
        use crate::{game::*, geometry::Vec3};
        let seat = Vec3(10.0, 64.5, 20.0);
        for (remote, riding) in [
            (None, Some(42)),
            (Some(777), Some(42)),
            (Some(4), None),
            (Some(47), Some(99)),
        ] {
            let mut game = Game {
                remote: remote.map(|protocol| Remote {
                    session: Some(protocol),
                }),
                boats: vec![Boat { id: 42 }],
                riding,
                player: None,
                previous_position: Vec3::ZERO,
                seat: &seat,
            };
            assert_eq!(inject(&mut game), core::ops::ControlFlow::Continue(()));
            assert_eq!(game.previous_position, Vec3::ZERO);
        }
    }
}
