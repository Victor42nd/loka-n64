#![cfg_attr(target_vendor = "nintendo64", no_std)]

#![cfg_attr(target_vendor = "nintendo64",feature(alloc_error_handler))]
#![cfg_attr(target_vendor = "nintendo64",feature(global_asm))]
#![cfg_attr(target_vendor = "nintendo64",feature(lang_items))]
#![cfg_attr(target_vendor = "nintendo64",feature(start))]

extern crate alloc;

mod bullet_system;
mod enemy_system;
mod player;
mod components;
mod entity;

use alloc::boxed::Box;
use alloc::vec::Vec;
use bullet_system::BulletSystem;
use enemy_system::EnemySystem;
use n64_math::Color;
use n64::{self, current_time_us, graphics, ipl3font, Controllers, Rng, audio};
use player::{Player, SHIP_SIZE};
use components::{movable_mut, char_drawable};

const BLUE: Color = Color::new(0b00001_00001_11100_1);
const RED: Color = Color::new(0b10000_00011_00011_1);

fn main() {
    // Todo maybe return n64 object that has funcs
    n64::init();

    let mut controllers = Box::new(Controllers::new());
    let mut player = Box::new(Player::new());
    let mut bullet_system = Box::new(BulletSystem::new());
    let mut enemy_system = Box::new(EnemySystem::new());
    let mut rng = Box::new(Rng::new_unseeded());

    /*let mut audio_buffer = {
        let mut buffer = Vec::new();
        buffer.resize_with(audio::BUFFER_NO_SAMPLES, Default::default);
        buffer.into_boxed_slice()
    };*/

    let mut time_used;
    let mut time_frame = current_time_us();
    let mut dt;

    enemy_system.spawn_enemy(&mut rng);
    enemy_system.spawn_enemy(&mut rng);
    enemy_system.spawn_enemy(&mut rng);
    enemy_system.spawn_enemy(&mut rng);
    enemy_system.spawn_enemy(&mut rng);
    enemy_system.spawn_enemy(&mut rng);

    loop {
        {
            let now = current_time_us();
            dt = (now - time_frame) as f32 / 1e6;
            time_frame = now;
        }

        time_used = current_time_us();

        {
            // Update

            controllers.update();

            enemy_system.update(&mut bullet_system, &mut player, &mut rng);

            player.update(&controllers, &mut bullet_system, &mut rng);

            bullet_system.update(&mut enemy_system, &mut player, &mut rng);

            movable_mut().simulate(dt);

            if player.is_dead() {
                break;
            }
        }

        /*{
            // Audio

            if !audio::all_buffers_are_full() {

                for (i, chunk) in audio_buffer.chunks_mut(128).enumerate() {
                    for sample in chunk {
                        if i % 2 == 0 {
                            *sample = 5000;
                        } else {
                            *sample = -5000;
                        }
                    }
                }

                audio::write_audio_blocking(&audio_buffer);
            }

            audio::update();
        }*/

        {
            // Draw

            graphics::clear_buffer();

            enemy_system.draw();

            char_drawable().draw();

            ipl3font::draw_number(300, 10, BLUE, player.score());
            ipl3font::draw_number(300, 215, BLUE, player.health());

            {
                let used_frame_time = current_time_us() - time_used;
                ipl3font::draw_number(200, 10, RED, used_frame_time as i32);
                ipl3font::draw_number(100, 10, RED, (dt * 1000.0 * 1000.0) as i32);
            }

            graphics::swap_buffers();
        }
    }

    loop {
        graphics::clear_buffer();
        ipl3font::draw_str(50, 10, RED, b"GAME OVER");
        graphics::swap_buffers();
    }
}

#[cfg(target_vendor = "nintendo64")]
#[global_allocator]
static ALLOC: n64_alloc::N64Alloc = n64_alloc::N64Alloc::INIT;

#[cfg(target_vendor = "nintendo64")]
#[start]
fn start(_argc: isize, _argv: *const *const u8) -> isize {
    main();
    0
}

#[cfg(target_vendor = "nintendo64")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {

    graphics::clear_buffer();
    ipl3font::draw_str(50, 10, RED, b"PANIC");
    graphics::swap_buffers();

    loop {}
}

#[cfg(target_vendor = "nintendo64")]
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    
    graphics::clear_buffer();
    ipl3font::draw_str(50, 10, RED, b"OUT OF MEMORY");
    graphics::swap_buffers();

    loop {}
}

#[cfg(target_vendor = "nintendo64")]
#[lang = "eh_personality"]
extern fn rust_eh_personality() {}

#[cfg(target_vendor = "nintendo64")]
global_asm!(include_str!("entrypoint.s"));