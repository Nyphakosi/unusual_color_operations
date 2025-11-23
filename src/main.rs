use std::{env, sync::{Arc, Mutex}, thread};
use image::{ColorType, DynamicImage, GenericImage, GenericImageView, Rgb, Rgba};
use unusual_color_operations as uco;

const IDENTITY: fn(f32)->f32 = |x| x;

fn main() {

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: input a file path");
        println!("Example: cargo run -- folder/imgname.png");
        println!("Press enter to end program");
        uco::inputstr();
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
        selection = uco::inputi8();
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
            reflect_angle = uco::inputf32();
        }
        3 => {
            println!("sample 1:");
            let tp00 = uco::inputf32();
            println!("target 1:");
            let tp01 = uco::inputf32();
            println!("sample 2:");
            let tp10 = uco::inputf32();
            println!("target 2:");
            let tp11 = uco::inputf32();
            two_point = ((tp00,tp01),(tp10,tp11));
        }
        4 => {
            println!("Input any number of points within the rectangle (0,0) to (360,360)");
            println!("Input point (-1,-1) to stop");
            loop {
                println!("sample:"); let px = uco::inputf32();
                println!("target:"); let py = uco::inputf32();
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

    let operation: Arc<dyn Fn(f32)->f32 + Send + Sync> = match selection {
        1 => Arc::new(uco::angle_reflect(reflect_angle)),
        2 => Arc::new(uco::linear_piece_two((120., 165.), (300., 285.))),
        3 => Arc::new(uco::linear_piece_two(two_point.0, two_point.1)),
        4 => Arc::new(uco::linear_piece_any(n_points)),
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
                            let hsv = uco::rgb_to_hsv(&pxl);
                            let new_hsv = uco::Hsv([inloop_operation(hsv.0[0]), hsv.0[1], hsv.0[2]]);
                            let new_rgb = uco::hsv_to_rgb(&new_hsv);
                            let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                            new_img_clone.lock().unwrap().put_pixel(x, y*core_count+y_inner, new_pixel);
                        }
                        (-2..=-1) => { // greater and lesser color conjugation
                            let new_rgb = unusual_color_operations::rgb_conjugate(&pxl, selection == -1);
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
                    let hsv = uco::rgb_to_hsv(&pxl);
                    let new_hsv = uco::Hsv([operation(hsv.0[0]), hsv.0[1], hsv.0[2]]);
                    let new_rgb = uco::hsv_to_rgb(&new_hsv);
                    let new_pixel = Rgba([new_rgb[0], new_rgb[1], new_rgb[2], pixel[3]]);
                    new_img.lock().unwrap().put_pixel(x, y, new_pixel);
                }
                (-2..=-1) => {
                    let new_rgb = unusual_color_operations::rgb_conjugate(&pxl, selection == -1);
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
    let mut outpath = file_path.clone();
    outpath.push_str("_output.png");
    new_img.lock().unwrap().save(outpath).unwrap();
    println!("Saved in {}ms", timer.elapsed().as_millis());

    println!("Press enter to end program");
    uco::inputstr();
}