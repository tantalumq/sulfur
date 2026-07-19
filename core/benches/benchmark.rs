use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    time::Duration,
};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use sulfur::ArchiveWriter;
use tempfile::tempdir;

#[allow(clippy::missing_panics_doc)]
pub fn benchmark_multiple_small_files(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("sulfur_bench");

    benchmark_group.sample_size(50);
    benchmark_group.measurement_time(Duration::from_secs(20));

    benchmark_group.bench_function("Multiple files pack (200 files x 10 KiB)", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().expect("Can't create temp directory");

                let source_dir = dir.path().join("source");
                fs::create_dir(&source_dir).expect("Can't create source directory");

                for i in 0..200 {
                    let file_path = source_dir.join(format!("file_{i}.txt"));
                    let mut file = File::create(file_path).expect("Can't create file");

                    let pattern = format!("File {i} content");

                    for _ in 0..(10240 / pattern.len()) {
                        file.write_all(pattern.as_bytes())
                            .expect("Can't write into file");
                    }
                    file.sync_all().expect("Can't sync file");
                }

                let target = dir.path().join("test.slf");

                (dir, source_dir, target)
            },
            |(_dir, source_dir, target)| {
                let file = File::create(target).expect("Can't create archive");
                let writer = BufWriter::new(file);
                let archive = ArchiveWriter::new(writer).expect("Can't create ArchiveWriter");

                archive.pack(&source_dir).expect("Can't pack");
            },
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, benchmark_multiple_small_files);
criterion_main!(benches);
