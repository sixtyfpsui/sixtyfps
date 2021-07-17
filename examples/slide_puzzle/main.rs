/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use sixtyfps::Model;
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

sixtyfps::include_modules!();

fn shuffle() -> Vec<i8> {
    fn is_solvable(positions: &[i8]) -> bool {
        // Same source as the flutter's slide_puzzle:
        // https://www.cs.bham.ac.uk/~mdr/teaching/modules04/java2/TilesSolvability.html

        let mut inversions = 0;
        for x in 0..positions.len() - 1 {
            let v = positions[x];
            inversions += positions[x + 1..].iter().filter(|x| **x >= 0 && **x < v).count();
        }
        //((blank on odd row from bottom) == (#inversions even))
        let blank_row = positions.iter().position(|x| *x == -1).unwrap() as usize / 4;
        inversions % 2 != blank_row % 2
    }

    let mut vec = ((-1)..15).into_iter().collect::<Vec<i8>>();
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    vec.shuffle(&mut rng);
    while !is_solvable(&vec) {
        vec.shuffle(&mut rng);
    }
    vec
}

struct AppState {
    pieces: Rc<sixtyfps::VecModel<Piece>>,
    main_window: sixtyfps::Weak<MainWindow>,
    /// An array of 16 values which represent a 4x4 matrix containing the piece number in that
    /// position. -1 is no piece.
    positions: Vec<i8>,
    auto_play_timer: sixtyfps::Timer,
    kick_animation_timer: sixtyfps::Timer,
    /// The speed in the x and y direction for the associated tile
    speed_for_kick_animation: [(f32, f32); 15],
    finished: bool,
}

impl AppState {
    fn set_pieces_pos(&self, p: i8, pos: i8) {
        if p >= 0 {
            self.pieces.set_row_data(
                p as usize,
                Piece { pos_y: (pos % 4) as _, pos_x: (pos / 4) as _, offset_x: 0., offset_y: 0. },
            );
        }
    }

    fn randomize(&mut self) {
        self.positions = shuffle();
        for (i, p) in self.positions.iter().enumerate() {
            self.set_pieces_pos(*p, i as _);
        }
        self.main_window.unwrap().set_moves(0);
        self.apply_tiles_left();
    }

    fn apply_tiles_left(&mut self) {
        let left = 15 - self.positions.iter().enumerate().filter(|(i, x)| *i as i8 == **x).count();
        self.main_window.unwrap().set_tiles_left(left as _);
        self.finished = left == 0;
    }

    fn piece_clicked(&mut self, p: i8) -> bool {
        let piece = self.pieces.row_data(p as usize);
        assert_eq!(self.positions[(piece.pos_x * 4 + piece.pos_y) as usize], p);

        // find the coordinate of the hole.
        let hole = self.positions.iter().position(|x| *x == -1).unwrap() as i8;
        let pos = (piece.pos_x * 4 + piece.pos_y) as i8;
        let sign = if pos > hole { -1 } else { 1 };
        if hole % 4 == piece.pos_y as i8 {
            self.slide(pos, sign * 4)
        } else if hole / 4 == piece.pos_x as i8 {
            self.slide(pos, sign)
        } else {
            self.speed_for_kick_animation[p as usize] = (
                if hole % 4 > piece.pos_y as i8 { 10. } else { -10. },
                if hole / 4 > piece.pos_x as i8 { 10. } else { -10. },
            );
            return false;
        };
        self.apply_tiles_left();
        self.main_window.upgrade().map(|x| x.set_moves(x.get_moves() + 1));
        true
    }

    fn slide(&mut self, pos: i8, offset: i8) {
        let mut swap = pos;
        while self.positions[pos as usize] != -1 {
            swap += offset;
            self.positions.swap(pos as usize, swap as usize);
            self.set_pieces_pos(self.positions[swap as usize] as _, swap);
        }
    }

    fn random_move(&mut self) {
        let mut rng = rand::thread_rng();
        let hole = self.positions.iter().position(|x| *x == -1).unwrap() as i8;
        let mut p;
        loop {
            p = rand::Rng::gen_range(&mut rng, 0..16);
            if hole == p {
                continue;
            } else if (hole % 4 == p % 4) || (hole / 4 == p / 4) {
                break;
            }
        }
        let p = self.positions[p as usize];
        self.piece_clicked(p);
    }

    /// Advance the kick animation
    fn kick_animation(&mut self) {
        /// update offset and speed, returns true if the animation is still running
        fn spring_animation(offset: &mut f32, speed: &mut f32) -> bool {
            const C: f32 = 0.3; // Constant = k/m
            const DAMP: f32 = 0.7;
            const EPS: f32 = 0.3;
            let acceleration = -*offset * C;
            *speed += acceleration;
            *speed *= DAMP;
            if *speed != 0. || *offset != 0. {
                *offset += *speed;
                if speed.abs() < EPS && offset.abs() < EPS {
                    *speed = 0.;
                    *offset = 0.;
                }
                true
            } else {
                false
            }
        }

        let mut has_animation = false;
        for idx in 0..15 {
            let mut p = self.pieces.row_data(idx);
            let ax = spring_animation(&mut p.offset_x, &mut self.speed_for_kick_animation[idx].0);
            let ay = spring_animation(&mut p.offset_y, &mut self.speed_for_kick_animation[idx].1);
            if ax || ay {
                self.pieces.set_row_data(idx, p);
                has_animation = true;
            }
        }
        if !has_animation {
            self.kick_animation_timer.stop();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    // This provides better error messages in debug mode.
    // It's disabled in release mode so it doesn't bloat up the file size.
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    console_error_panic_hook::set_once();

    let main_window = MainWindow::new();
    let state = Rc::new(RefCell::new(AppState {
        pieces: Rc::new(sixtyfps::VecModel::<Piece>::from(vec![Piece::default(); 15])),
        main_window: main_window.as_weak(),
        positions: vec![],
        auto_play_timer: Default::default(),
        kick_animation_timer: Default::default(),
        speed_for_kick_animation: Default::default(),
        finished: false,
    }));
    state.borrow_mut().randomize();
    main_window.set_pieces(sixtyfps::ModelHandle::new(state.borrow().pieces.clone()));

    let state_copy = state.clone();
    main_window.on_piece_clicked(move |p| {
        state_copy.borrow().auto_play_timer.stop();
        state_copy.borrow().main_window.unwrap().set_auto_play(false);
        if state_copy.borrow().finished {
            return;
        }
        if !state_copy.borrow_mut().piece_clicked(p as i8) {
            let state_weak = Rc::downgrade(&state_copy);
            state_copy.borrow().kick_animation_timer.start(
                sixtyfps::TimerMode::Repeated,
                std::time::Duration::from_millis(16),
                move || {
                    if let Some(state) = state_weak.upgrade() {
                        state.borrow_mut().kick_animation();
                    }
                },
            );
        }
    });

    let state_copy = state.clone();
    main_window.on_reset(move || {
        state_copy.borrow().auto_play_timer.stop();
        state_copy.borrow().main_window.unwrap().set_auto_play(false);
        state_copy.borrow_mut().randomize();
    });

    let state_copy = state.clone();
    main_window.on_enable_auto_mode(move |enabled| {
        if enabled {
            let state_weak = Rc::downgrade(&state_copy);
            state_copy.borrow().auto_play_timer.start(
                sixtyfps::TimerMode::Repeated,
                std::time::Duration::from_millis(200),
                move || {
                    if let Some(state) = state_weak.upgrade() {
                        state.borrow_mut().random_move();
                    }
                },
            );
        } else {
            state_copy.borrow().auto_play_timer.stop();
        }
    });
    main_window.run();
}
