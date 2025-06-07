use std::{env, io, sync::{Arc, Mutex}, thread};

use image::{ColorType, DynamicImage, GenericImage, GenericImageView, ImageBuffer, Rgb, Rgba};

// rgb↔hsv conversion functions taken from https://gist.github.com/bmgxyz/a5b5b58e492cbca099b468eddd04cc97

struct Hsv([f32; 3]);

fn rgb_to_hsv(pixel: &Rgb<u8>) -> Hsv {
    let [r, g, b] = pixel.0;
    let big_m = *[r, g, b].iter().max().unwrap() as f32 / 255.;
    let little_m = *[r, g, b].iter().min().unwrap() as f32 / 255.;
    let c = big_m - little_m;
    let s = (c / big_m) * 100.;
    let (little_r, little_g, little_b) = (r as f32 / 255., g as f32 / 255., b as f32 / 255.);
    let (big_r, big_g, big_b) = (
        (big_m - little_r) / c,
        (big_m - little_g) / c,
        (big_m - little_b) / c,
    );
    let h_prime = match big_m {
        x if x == little_m => 0.,
        x if x == little_r => big_b - big_g,
        x if x == little_g => 2. + big_r - big_b,
        x if x == little_b => 4. + big_g - big_r,
        _ => unreachable!(),
    };
    let h = h_prime / 6. * 360.;
    let v = big_m * 100.;
    Hsv([h, s, v])
}

fn hsv_to_rgb(pixel: &Hsv) -> Rgb<u8> {
    let [hue, saturation, value] = [pixel.0[0], pixel.0[1] / 100., pixel.0[2] / 100.];
    let max = value;
    let c = saturation * value;
    let min = max - c;
    let h_prime = if hue >= 300. {
        (hue - 360.) / 60.
    } else {
        hue / 60.
    };
    let (r, g, b) = match h_prime {
        x if -1. <= x && x < 1. => {
            if h_prime < 0. {
                (max, min, min - h_prime * c)
            } else {
                (max, min + h_prime * c, min)
            }
        }
        x if 1. <= x && x < 3. => {
            if h_prime < 2. {
                (min - (h_prime - 2.) * c, max, min)
            } else {
                (min, max, min + (h_prime - 2.) * c)
            }
        }
        x if 3. <= x && x < 5. => {
            if h_prime < 4. {
                (min, min - (h_prime - 4.) * c, max)
            } else {
                (min + (h_prime - 4.) * c, min, max)
            }
        }
        _ => unreachable!(),
    };
    Rgb([(r * 255.) as u8, (g * 255.) as u8, (b * 255.) as u8])
}

// hue reflection algorithm
fn hsv_reflect(pixel: &Hsv, reflect_angle: f32) -> Hsv {
    let [hue, saturation, value] = [pixel.0[0], pixel.0[1], pixel.0[2]];

    // for a hue angle C and reflection angle A
    // align angle to the 0 degree line by subtracting reflection angle
    // subtract from 360 to flip, add back reflection angle to realign
    // output angle is 360-(C-A)+A mod 360
    // or, 360-C+2A mod 360
    // and with algebra, 2A-C mod 360
    let angle = (2.0*reflect_angle - hue).rem_euclid(360.0);

    Hsv([angle, saturation, value])
}

fn rgb_conjugate(pixel: &Rgb<u8>) -> Rgb<u8> {
    let channels = vec![pixel.0[0], pixel.0[1], pixel.0[2]];
    // idk how to do this better
    let smallest_channel_value = *channels.iter().min().unwrap();
    let smallest_channel = channels.iter().position(|&p| p == smallest_channel_value).unwrap();

    let mut new_pixel = Rgb([0; 3]);

    match smallest_channel {
        0 => {new_pixel.0[0] = pixel.0[0]; new_pixel.0[1] = pixel.0[2]; new_pixel.0[2] = pixel.0[1]},
        1 => {new_pixel.0[0] = pixel.0[2]; new_pixel.0[1] = pixel.0[1]; new_pixel.0[2] = pixel.0[0]},
        2 => {new_pixel.0[0] = pixel.0[1]; new_pixel.0[1] = pixel.0[0]; new_pixel.0[2] = pixel.0[2]},
        _ => (),
    }

    return new_pixel
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: input a file path");
        println!("Example: cargo run -- folder/imgname.png");
        return;
    }

    let timer = std::time::Instant::now();

    let core_count: u32 = num_cpus::get() as u32;
    let file_path: &String = &args[1];
    let img = Arc::new(image::open(file_path).expect("Failed to open image"));
    let (width, height) = img.dimensions();
    let new_img = Arc::new(Mutex::new(DynamicImage::new(width, height, ColorType::Rgba8)));

    println!("Image loaded in {}ms", timer.elapsed().as_millis());

    let mut selection: u8 = 0;
    loop {
        println!("Select Operation");
        println!("1: Hue Reflection, reflect the color wheel around an angle");
        println!("2: Color Conjugate, swap the greater two color channels");
        selection = inputu8();
        match selection {
            1..=2 => break,
            _ => continue, // absolutely do not allow invalid inputs
        }
    }
    let selection = selection;

    let mut reflect_angle: f32 = 0.0;
    if selection == 1 {
        println!("Input Reflection Angle:");
        reflect_angle = inputf32();
    }
    let reflect_angle = reflect_angle;

    let mut handles: Vec<thread::JoinHandle<()>> = Vec::new();

    println!("Processing...");
    // process main image
    for y in 0..height/core_count {
        for y_inner in 0..core_count { // divide image rows by number of cores in device
            let img_clone = Arc::clone(&img);
            let new_img_clone = Arc::clone(&new_img);
            handles.push(thread::spawn(move || {
                for x in 0..width {
                    let pixel = img_clone.get_pixel(x, y*core_count+y_inner);
                    match selection {
                        1 => { // hue reflection
                            let pxl: Rgb<u8> = Rgb([pixel[0], pixel[1], pixel[2]]);
                            let hsv = rgb_to_hsv(&pxl);
                            let new_hsv = hsv_reflect(&hsv, reflect_angle);
                            let new_rgb = hsv_to_rgb(&new_hsv);
                            let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                            new_img_clone.lock().unwrap().put_pixel(x, y*core_count+y_inner, new_pixel);
                        }
                        2 => { // color conjugation
                            let pxl: Rgb<u8> = Rgb([pixel[0], pixel[1], pixel[2]]);
                            let new_rgb = rgb_conjugate(&pxl);
                            let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                            new_img_clone.lock().unwrap().put_pixel(x, y*core_count+y_inner, new_pixel);
                        }
                        _ => unreachable!(),
                    }
                }
            }));
        }
    }
    // process remainder of image
    for y in height/core_count*core_count..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let pxl: Rgb<u8> = Rgb([pixel[0], pixel[1], pixel[2]]);
            let hsv = rgb_to_hsv(&pxl);

            let new_hsv = hsv_reflect(&hsv, reflect_angle);
            let new_rgb = hsv_to_rgb(&new_hsv);
            let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
            new_img.lock().unwrap().put_pixel(x, y, new_pixel);
        }
    }

    for handle in handles {
        handle.join().unwrap();
    }
    println!("Done in {}ms", timer.elapsed().as_millis());

    let timer = std::time::Instant::now();
    new_img.lock().unwrap().save("output.png").unwrap();
    println!("Saved in {}ms", timer.elapsed().as_millis());

    println!("Press enter to end program");
    inputstr();
}

fn inputf32() -> f32 {
    loop {
        let mut value = String::new();

        io::stdin()
            .read_line(&mut value)
            .expect("Failed to read line");

        match value.trim().parse() {
            Ok(num) => return num,
            Err(_) => continue,
        };
    }
}

fn inputu8() -> u8 {
    loop {
        let mut value = String::new();

        io::stdin()
            .read_line(&mut value)
            .expect("Failed to read line");

        match value.trim().parse() {
            Ok(num) => return num,
            Err(_) => continue,
        };
    }
}

fn inputstr() -> String {
    loop {
        let mut value = String::new();

        io::stdin()
            .read_line(&mut value)
            .expect("Failed to read line");

        match value.trim().parse() {
            Ok(num) => return num,
            Err(_) => continue,
        };
    }
}