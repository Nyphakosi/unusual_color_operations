use std::{
    env,
    sync::{Arc, Mutex, mpsc::{Sender as Tx, Receiver as Rx}},
    thread::ScopedJoinHandle,
};
use image::{Rgb, Rgba, buffer::{Pixels, PixelsMut}};
use unusual_color_operations as uco;

const IDENTITY: fn(f32)->f32 = |x| x;
type Amrx<T> = Arc<Mutex<Rx<T>>>;

fn thrpool<T: Send, U: Send, W: Send, R>(
    nthr: usize,
    worker: impl Sync + Fn(Amrx<T>, Tx<U>) -> W,
    manager: impl for<'scope>
        FnOnce(Tx<T>, Rx<U>, Box<[ScopedJoinHandle<'scope, W>]>) -> R,
) -> R {
    let worker = &worker;
    std::thread::scope(|scope| {
        let (task_tx, task_rx) = std::sync::mpsc::channel();
        let (res_tx, res_rx) = std::sync::mpsc::channel();
        let task_rx = Arc::new(Mutex::new(task_rx));
        let handles: Box<[_]> = (0..nthr).map(|_| {
            let (task_rx, res_tx) = (Arc::clone(&task_rx), res_tx.clone());
            scope.spawn(move || worker(task_rx, res_tx))
        }).collect();
        drop((task_rx, res_tx));
        manager(task_tx, res_rx, handles)
    })
}

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
    // let (width, height) = img.dimensions();
    // let new_img = Arc::new(Mutex::new(DynamicImage::new(width, height, ColorType::Rgba8)));

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

    let timer = std::time::Instant::now();

    let operation: Arc<dyn Fn(f32)->f32 + Send + Sync> = match selection {
        1 => Arc::new(uco::angle_reflect(reflect_angle)),
        2 => Arc::new(uco::linear_piece_two((120., 165.), (300., 285.))),
        3 => Arc::new(uco::linear_piece_two(two_point.0, two_point.1)),
        4 => Arc::new(uco::linear_piece_any(n_points)),
        _ => Arc::new(IDENTITY),
    };

    println!("Processing...");

    let isrc = img.as_ref().clone().into_rgba8();
    let mut ides = image::RgbaImage::new(img.width(), img.height());
    let op = &operation;
    thrpool(
        core_count as usize,
        |task_rx: Amrx<Option<(Pixels<_>, PixelsMut<_>)>>, _res_tx: Tx<()>| -> () {
            while let Some((row_in, row_out)) =
                task_rx.lock().expect("wtf").recv().expect("wtf")
            {
                for (px_in, px_out) in <Pixels<'_, Rgba<u8>> as Iterator>::zip(row_in, row_out) {
                    let pxl = Rgb([px_in[0], px_in[1], px_in[2]]);
                    *px_out = match selection {
                        (1..=4) => {
                            let hsv = uco::rgb_to_hsv(&pxl);
                            let new_hsv = uco::Hsv([op(hsv.0[0]), hsv.0[1], hsv.0[2]]);
                            let new_rgb = uco::hsv_to_rgb(&new_hsv);
                            Rgba([new_rgb[0], new_rgb[1], new_rgb[2], px_in[3]])
                        }
                        (-2..=-1) => {
                            let new_rgb = unusual_color_operations::rgb_conjugate(&pxl, selection == -1);
                            Rgba([new_rgb[0], new_rgb[1], new_rgb[2], px_in[3]])
                        }
                        _ => unreachable!(),
                    }
                }
            }
        },
        |task_tx, _res_rx, handles| {
            for row_pair in isrc.rows().zip(ides.rows_mut()) {
                task_tx.send(Some(row_pair)).expect("wtf");
            }
            for _ in 0..handles.len() {
                task_tx.send(None).expect("wtf");
            }
            for h in handles.into_iter() {
                () = h.join()?;
            }
            Ok::<_, Box<dyn std::any::Any + Send>>(())
        },
    ).expect("Failed??");

    println!("Done in {}ms", timer.elapsed().as_millis());

    let timer = std::time::Instant::now();
    let mut outpath = file_path.clone();
    outpath.push_str("_output.png");
    ides.save(outpath).unwrap();
    println!("Saved in {}ms", timer.elapsed().as_millis());

    println!("Press enter to end program");
    uco::inputstr();
}
