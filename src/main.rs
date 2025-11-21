use std::{env, io, sync::{Arc, Mutex}, thread};

use image::{ColorType, DynamicImage, GenericImage, GenericImageView, Rgb, Rgba};

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
        x if (-1. ..1.).contains(&x) => {
            if h_prime < 0. {
                (max, min, min - h_prime * c)
            } else {
                (max, min + h_prime * c, min)
            }
        }
        x if (1. ..3.).contains(&x) => {
            if h_prime < 2. {
                (min - (h_prime - 2.) * c, max, min)
            } else {
                (min, max, min + (h_prime - 2.) * c)
            }
        }
        x if (3. ..5.).contains(&x) => {
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
fn angle_reflect(reflect_angle: f32) -> impl Fn(f32)->f32 + Send + 'static {
    // for a hue angle C and reflection angle A
    // align angle to the 0 degree line by subtracting reflection angle
    // subtract from 360 to flip, add back reflection angle to realign
    // output angle is 360-(C-A)+A mod 360
    // or, 360-C+2A mod 360
    // and with algebra, 2A-C mod 360
    move |x| (2.0*reflect_angle - x).rem_euclid(360.0)
}
fn linear_piece_any(points: Vec<(f32, f32)>) -> impl Fn(f32)->f32 + Send + 'static {
    let mut points = points.clone();
    if points.is_empty() {
        points.push((180., 180.));
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0)); // order by x coordinate
    points.insert(0, (points[points.len()-1].0 - 360.0, points[points.len()-1].1 - 360.0)); // shift the last point leftward to before the modulo bounds
    points.push((points[0].0 + 360.0, points[0].1 + 360.0)); // shift the first point rightward to after the modulo bounds
    let len = points.len();
    let points = points;

    let mut slopes: Vec<(f32,f32)> = Vec::new(); // the first and last slopes go over the modulo
    let mut prev_point: (f32, f32) = points[0];
    for point in &points[1..len] { // compute the slope between every point, and bias using the first point as reference
        let slope: f32 = (point.1 - prev_point.1) / (point.0 - prev_point.0);
        let bias = prev_point.1 - slope * prev_point.0;
        slopes.push((slope, bias));
        prev_point = *point;
    }
    let slopes = slopes;

    move |x| {
        let mut index: usize = 0;
        for i in 0..len-1 {
            if x >= points[i].0 && x < points[i+1].0 {
                index = i; break;
            }
        }
        (x * slopes[index].0 + slopes[index].1).rem_euclid(360.0)
    }
}
fn linear_piece_two(p1: (f32, f32), p2: (f32, f32)) -> impl Fn(f32)->f32 + Send + 'static {
    // takes in two points and creates a partwise linear function between them both ways, modulo 360
    let (a,b) = if p1.0 < p2.0 {(p1, p2)} else {(p2, p1)};
    let slope_lowhigh = (b.1-a.1)/(b.0-a.0);
    let slope_highlow = (a.1+360.-b.1)/(a.0+360.-b.0);
    let lowhigh_bias = a.1 - slope_lowhigh * a.0;
    let highlow_bias_first = a.1 - slope_highlow * a.0;
    let highlow_bias_second = b.1 - slope_highlow * b.0;
    move |x| {
        if x < a.0 {
            (x * slope_highlow + highlow_bias_first).rem_euclid(360.)
        } else if (a.0 ..b.0).contains(&x) {
            (x * slope_lowhigh + lowhigh_bias).rem_euclid(360.)
        } else {
            (x * slope_highlow + highlow_bias_second).rem_euclid(360.)
        }
    }
}

// swap the two largest or smallest color channels
// true for largest
// false for smallest
fn rgb_conjugate(pixel: &Rgb<u8>, minmax: bool) -> Rgb<u8> {
    let mut sorted: Vec<u8> = pixel.0.to_vec();
    sorted.sort();
    let sorted = sorted;

    let channel_position = pixel.0.iter().position(|&p| p == sorted[match minmax {true => 0, false => 2}]).unwrap();
    match channel_position {
        0 => Rgb([pixel.0[0], pixel.0[2], pixel.0[1]]), // swaps g and b
        1 => Rgb([pixel.0[2], pixel.0[1], pixel.0[0]]), // swaps r and b
        2 => Rgb([pixel.0[1], pixel.0[0], pixel.0[2]]), // swaps r and g
        _ => Rgb([0, 0, 0]),
    }
}

const IDENTITY: fn(f32)->f32 = |x| x;

fn main() {

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: input a file path");
        println!("Example: cargo run -- folder/imgname.png");
        inputstr();
        return;
    }

    let timer = std::time::Instant::now();

    let core_count: u32 = num_cpus::get() as u32;
    let file_path: &String = &args[1];
    let img = Arc::new(image::open(file_path).expect("Failed to open image"));
    let (width, height) = img.dimensions();
    let new_img = Arc::new(Mutex::new(DynamicImage::new(width, height, ColorType::Rgba8)));

    println!("Image loaded in {}ms", timer.elapsed().as_millis());

    let mut selection: i8;
    loop {
        println!("Select Operation");
        println!("1: Hue Reflection, reflect the color wheel around an angle");
        println!("2: Phos' Operation, 120°→165° and 300°→285°");
        println!("3: Two Point Shift, same as above but for any two input points");
        println!("4: N Point Shift, same as above but for any input points");
        println!("-1: Greater Color Conjugate, swap the larger two color channels");
        println!("-2: Lesser Color Conjugate, swap the smaller two color channels");
        selection = inputi8();
        match selection {
            -2..=4 => break,
            _ => continue, // absolutely do not allow invalid inputs
        }
    }
    let selection = selection;

    let mut reflect_angle: f32 = 0.0;
    let mut two_point: ((f32,f32),(f32,f32)) = ((0.0,0.0),(0.0,0.0));
    let mut n_points: Vec<(f32, f32)> = Vec::new();
    match selection {
        1 => {
            println!("Input Reflection Angle:");
            reflect_angle = inputf32();
        }
        3 => {
            println!("Input start point 1");
            let tp00 = inputf32();
            println!("Input end point 1");
            let tp01 = inputf32();
            println!("Input start point 2");
            let tp10 = inputf32();
            println!("Input end point 2");
            let tp11 = inputf32();
            two_point = ((tp00,tp01),(tp10,tp11));
        }
        4 => {
            println!("Input any number of points within the rectangle (0,0) to (360,360)");
            println!("Input point (-1,-1) to stop");
            loop {
                println!("sample:"); let px = inputf32();
                println!("target:"); let py = inputf32();
                if px == -1. && py == -1. { break }
                n_points.push((px,py));
            }
        }
        _ => ()
    } 
    let reflect_angle = reflect_angle;
    let two_point = two_point;

    let mut handles: Vec<thread::JoinHandle<()>> = Vec::new();

    let timer = std::time::Instant::now();

    let operation: Arc<dyn Fn(f32)->f32 + Send + Sync + 'static> = match selection {
        1 => Arc::new(angle_reflect(reflect_angle)),
        2 => Arc::new(linear_piece_two((120., 165.), (300., 285.))),
        3 => Arc::new(linear_piece_two(two_point.0, two_point.1)),
        4 => Arc::new(linear_piece_any(n_points)),
        _ => Arc::new(IDENTITY),
    };

    println!("Processing...");
    // process main image
    for y in 0..height/core_count {
        for y_inner in 0..core_count { // divide image rows by number of cores in device
            let img_clone = Arc::clone(&img);
            let new_img_clone = Arc::clone(&new_img);
            let inloop_operation = operation.clone();
            handles.push(thread::spawn(move || {
                for x in 0..width {
                    let pixel = img_clone.get_pixel(x, y*core_count+y_inner);
                    let pxl: Rgb<u8> = Rgb([pixel[0], pixel[1], pixel[2]]);
                    match selection {
                        (1..=4) => { // hue reflection, phos' operation, two point shift
                            let hsv = rgb_to_hsv(&pxl);
                            let new_hsv = Hsv([inloop_operation(hsv.0[0]), hsv.0[1], hsv.0[2]]);
                            let new_rgb = hsv_to_rgb(&new_hsv);
                            let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                            new_img_clone.lock().unwrap().put_pixel(x, y*core_count+y_inner, new_pixel);
                        }
                        (-2..=-1) => { // greater and lesser color conjugation
                            let new_rgb = rgb_conjugate(&pxl, selection == -1);
                            let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                            new_img_clone.lock().unwrap().put_pixel(x, y*core_count+y_inner, new_pixel);
                        }
                        _ => unreachable!()
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
            match selection {
                (1..=4) => {
                    let hsv = rgb_to_hsv(&pxl);
                    let new_hsv = Hsv([operation(hsv.0[0]), hsv.0[1], hsv.0[2]]);
                    let new_rgb = hsv_to_rgb(&new_hsv);
                    let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                    new_img.lock().unwrap().put_pixel(x, y, new_pixel);
                }
                (-2..=-1) => {
                    let new_rgb = rgb_conjugate(&pxl, selection == -1);
                    let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                    new_img.lock().unwrap().put_pixel(x, y, new_pixel);
                }
                _ => unreachable!(),
            }
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

fn inputi8() -> i8 {
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