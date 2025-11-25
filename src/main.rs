use std::{
    env,
    sync::Arc,
};
use image::Rgba;
use unusual_color_operations as uco;

const IDENTITY: fn(f32)->f32 = |x| x;

fn main() {

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: input a file path");
        println!("Example: cargo run -- folder/imgname.png");
        println!("Press enter to end program");
        uco::input::<String>();
        return;
    }

    let timer = std::time::Instant::now();

    let file_path: &String = &args[1];
    let img = Arc::new(image::open(file_path).expect("Failed to open image"));

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
        selection = uco::input::<i8>();
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
            reflect_angle = uco::input::<f32>();
        }
        3 => {
            println!("sample 1:");
            let tp00 = uco::input::<f32>();
            println!("target 1:");
            let tp01 = uco::input::<f32>();
            println!("sample 2:");
            let tp10 = uco::input::<f32>();
            println!("target 2:");
            let tp11 = uco::input::<f32>();
            two_point = ((tp00,tp01),(tp10,tp11));
        }
        4 => {
            println!("Input any number of points within the rectangle (0,0) to (360,360)");
            println!("Input point (-1,-1) to stop");
            loop {
                println!("sample:"); let px = uco::input::<f32>();
                println!("target:"); let py = uco::input::<f32>();
                if px == -1. && py == -1. { break }
                n_points.push((px,py));
            }
        }
        _ => ()
    } 
    let reflect_angle = reflect_angle;
    let two_point = two_point;

    let timer = std::time::Instant::now();

    let op: Arc<dyn Fn(f32)->f32 + Send + Sync> = match selection {
        1 => Arc::new(uco::angle_reflect(reflect_angle)),
        2 => Arc::new(uco::linear_piece_two((120., 165.), (300., 285.))),
        3 => Arc::new(uco::linear_piece_two(two_point.0, two_point.1)),
        4 => Arc::new(uco::linear_piece_any(n_points)),
        _ => Arc::new(IDENTITY),
    };
    let operation: Arc<dyn Fn(Rgba<u8>)->Rgba<u8> + Send + Sync> = match selection {
        (1..=4) => { 
            Arc::new(uco::process_hue(Arc::clone(&op)))
        }
        (-2..=-1) => {
            Arc::new(uco::rgb_conjugate_wrapper(selection == -1))
        }
        _ => unreachable!(),
    };

    println!("Processing...");

    let isrc = img.as_ref().clone().into_rgba8();
    let ides = uco::process_image(&isrc, operation);

    println!("Done in {}ms", timer.elapsed().as_millis());

    let timer = std::time::Instant::now();
    let mut outpath = file_path.clone();
    outpath.push_str("_output.png");
    ides.save(outpath).unwrap();
    println!("Saved in {}ms", timer.elapsed().as_millis());

    println!("Press enter to end program");
    uco::input::<String>();
}
