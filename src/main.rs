mod utils;
use std::{
    fs,
    process::{self, Stdio},
};

use indicatif::{ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

fn main() {
    let paths = read_csv_paths("/Users/brendi/Sync/OHLCData/Stocks/30min", None);

    let tp = rayon::ThreadPoolBuilder::new()
        .num_threads(6)
        .build()
        .expect("Thread pool to build");

    let pb = ProgressBar::new(paths.len() as u64).with_message("none");

    let sty = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} processing: {msg}",
    )
    .expect("progress style template to parse")
    .progress_chars("##-");

    pb.set_style(sty.clone());

    let vec_of_responses = tp.install(|| {
        let responses = paths
            .into_par_iter()
            .map(|p| {
                let ticker = ticker_from_path(&p);
                let proc = process::Command::new("just")
                    .arg("run-proc")
                    .arg(&p)
                    .arg("0")
                    .arg("10")
                    .arg("0.0")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .unwrap();

                let out = String::from_utf8(proc.stdout).unwrap();
                pb.set_message(ticker);
                pb.inc(1);
                out
            })
            .filter(|o| !o.is_empty())
            .collect::<Vec<_>>();
        responses
    });

    pb.finish();

    // write to a csv file
    write_to_csv(vec_of_responses).unwrap();
}

fn write_to_csv(results: Vec<String>) -> anyhow::Result<()> {
    // export to csv.
    let mut wtr = csv::Writer::from_path("cool.csv")?;
    wtr.write_record([
        "ticker",
        "start_equity",
        "end_equity",
        "num_years",
        "total_plpc",
        "avg_plpc",
        "avg_vol",
    ])?;
    for r in results {
        let values = r.split(',').collect::<Vec<_>>();
        wtr.write_record(values)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn ticker_from_path(path: &str) -> String {
    path.split('/')
        .next_back()
        .unwrap()
        .split("_")
        .collect::<Vec<_>>()[0]
        .to_string()
}

pub fn read_csv_paths(path: &str, ticker: Option<String>) -> Vec<String> {
    let csv_path_dir = fs::read_dir(path).unwrap();
    csv_path_dir
        .into_iter()
        .filter(|p| p.is_ok())
        .filter_map(|v| {
            let v = v.unwrap();
            if v.file_type().unwrap().is_file() {
                Some(v.path())
            } else {
                None
            }
        })
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| {
            let ticker_name = ticker_from_path(p);
            if let Some(ticker) = &ticker {
                ticker_name.as_str() == ticker
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
}
