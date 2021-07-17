/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#[allow(dead_code)]
fn main() {
    use sixtyfps::Model;

    let main_window = MainWindow::new();

    // Fetch the tiles from the model
    let mut tiles: Vec<TileData> = main_window.get_memory_tiles().iter().collect();
    // Duplicate them to ensure that we have pairs
    tiles.extend(tiles.clone());

    // Randomly mix the tiles
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    tiles.shuffle(&mut rng);

    // ANCHOR: game_logic
    // Assign the shuffled Vec to the model property
    let tiles_model = std::rc::Rc::new(sixtyfps::VecModel::from(tiles));
    main_window.set_memory_tiles(sixtyfps::ModelHandle::new(tiles_model.clone()));

    let main_window_weak = main_window.as_weak();
    main_window.on_check_if_pair_solved(move || {
        let mut flipped_tiles =
            tiles_model.iter().enumerate().filter(|(_, tile)| tile.image_visible && !tile.solved);

        if let (Some((t1_idx, mut t1)), Some((t2_idx, mut t2))) =
            (flipped_tiles.next(), flipped_tiles.next())
        {
            let is_pair_solved = t1 == t2;
            if is_pair_solved {
                t1.solved = true;
                tiles_model.set_row_data(t1_idx, t1.clone());
                t2.solved = true;
                tiles_model.set_row_data(t2_idx, t2.clone());
            } else {
                let main_window = main_window_weak.unwrap();
                main_window.set_disable_tiles(true);
                let tiles_model = tiles_model.clone();
                sixtyfps::Timer::single_shot(std::time::Duration::from_secs(1), move || {
                    main_window.set_disable_tiles(false);
                    t1.image_visible = false;
                    tiles_model.set_row_data(t1_idx, t1);
                    t2.image_visible = false;
                    tiles_model.set_row_data(t2_idx, t2);
                });
            }
        }
    });

    main_window.run();
    // ANCHOR_END: game_logic
}
sixtyfps::sixtyfps! {
    struct TileData := {
        image: image,
        image_visible: bool,
        solved: bool,
    }

    MemoryTile := Rectangle {
        callback clicked;
        property <bool> open_curtain;
        property <bool> solved;
        property <image> icon;

        height: 64px;
        width: 64px;
        background: solved ? #34CE57 : #3960D5;
        animate background { duration: 800ms; }

        Image {
            source: icon;
            width: parent.width;
            height: parent.height;
        }

        // Left curtain
        Rectangle {
            background: #193076;
            width: open_curtain ? 0px : (parent.width / 2);
            height: parent.height;
            animate width { duration: 250ms; easing: ease-in; }
        }

        // Right curtain
        Rectangle {
            background: #193076;
            x: open_curtain ? parent.width : (parent.width / 2);
            width: open_curtain ? 0px : (parent.width / 2);
            height: parent.height;
            animate width { duration: 250ms; easing: ease-in; }
            animate x { duration: 250ms; easing: ease-in; }
        }

        TouchArea {
            clicked => {
                // Delegate to the user of this element
                root.clicked();
            }
        }
    }
    // ANCHOR: mainwindow_interface
    MainWindow := Window {
        width: 326px;
        height: 326px;

        callback check_if_pair_solved(); // Added
        property <bool> disable_tiles; // Added

        property <[TileData]> memory_tiles: [
           { image: @image-url("icons/at.png") },
    // ANCHOR_END: mainwindow_interface
           { image: @image-url("icons/balance-scale.png") },
           { image: @image-url("icons/bicycle.png") },
           { image: @image-url("icons/bus.png") },
           { image: @image-url("icons/cloud.png") },
           { image: @image-url("icons/cogs.png") },
           { image: @image-url("icons/motorcycle.png") },
           { image: @image-url("icons/video.png") },
        ];
        // ANCHOR: tile_click_logic
        for tile[i] in memory_tiles : MemoryTile {
            x: mod(i, 4) * 74px;
            y: floor(i / 4) * 74px;
            width: 64px;
            height: 64px;
            icon: tile.image;
            open_curtain: tile.image_visible || tile.solved;
            // propagate the solved status from the model to the tile
            solved: tile.solved;
            clicked => {
                // old: tile.image_visible = !tile.image_visible;
                // new:
                if (!root.disable_tiles) {
                    tile.image_visible = !tile.image_visible;
                    root.check_if_pair_solved();
                }
            }
        }
        // ANCHOR_END: tile_click_logic
    }
}
