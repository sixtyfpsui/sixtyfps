/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#[allow(dead_code)]
// ANCHOR: tiles
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

    // Assign the shuffled Vec to the model property
    let tiles_model = std::rc::Rc::new(sixtyfps::VecModel::from(tiles));
    main_window.set_memory_tiles(sixtyfps::ModelHandle::new(tiles_model.clone()));

    main_window.run();
}
// ANCHOR_END: tiles
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
    MainWindow := Window {
        width: 326px;
        height: 326px;

        property <[TileData]> memory_tiles: [
           { image: @image-url("icons/at.png") },
           { image: @image-url("icons/balance-scale.png") },
           { image: @image-url("icons/bicycle.png") },
           { image: @image-url("icons/bus.png") },
           { image: @image-url("icons/cloud.png") },
           { image: @image-url("icons/cogs.png") },
           { image: @image-url("icons/motorcycle.png") },
           { image: @image-url("icons/video.png") },
        ];
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
                tile.image_visible = !tile.image_visible;
            }
        }
    }
}
