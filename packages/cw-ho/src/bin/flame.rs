use cw_ho::Cli;
use pprof::ProfilerGuard;
use std::fs::File;
use std::io::Write;
use std::thread;
use std::time::Duration;

// TODO: use perf: https://nnethercote.github.io/perf-book/profiling.html
// TODO: implement demo usage for this 

fn main() {
    let guard: ProfilerGuard<'static> = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();

    cw_ho::start(
        Cli {
            command: cw_ho::Commands::Start { port: None },
            config: "".to_string(),
            log_level: "debug".to_string(),
        },
        None,
    ).unwrap();

    match guard.report().build() {
        Ok(report) => {
            // Create flamegraph file
            let mut flamegraph_file = File::create("flamegraph.svg").unwrap();
            if let Err(e) = report.flamegraph(&mut flamegraph_file) {
                eprintln!("Failed to generate flamegraph: {}", e);
                return;
            }

            println!("Flamegraph generated successfully!");
        }
        Err(e) => {
            eprintln!("Failed to generate report: {}", e);
        }
    };
}
