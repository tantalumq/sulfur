use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Duration,
};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use sulfur::{ArchiveWriter, archive_path};
use tempfile::tempdir;

#[allow(clippy::missing_panics_doc)]
pub fn benchmark_single_file_pack_real(c: &mut Criterion) {
    let dir = tempdir().expect("Can't create temp directory");

    let mut benchmark_group = c.benchmark_group("sulfur_bench");

    benchmark_group.sample_size(25);
    benchmark_group.measurement_time(Duration::from_secs(30));

    benchmark_group.bench_function("single file pack (100 MiB)", |b| {
        b.iter_batched(
            || {
                let source = dir.path().join("test.txt");
                let mut file = File::create(&source).expect("Can't create source file");

                for i in 0..100 {
                    let pattern = format!("Line {i}: repeating content here");
                    for _ in 0..(1024 * 1024 / pattern.len()) {
                        file.write_all(pattern.as_bytes())
                            .expect("Can't write randomized buffer into file");
                    }
                }
                file.sync_all().expect("Can't sync file");

                let mut target = dir.path().join("test.slf");

                target = archive_path(&source, &target).expect("Can't get path for archive");
                (source, target)
            },
            |(source, target)| {
                let file = File::create(target).expect("Can't create archive");
                let writer = BufWriter::new(file);
                let archive = ArchiveWriter::new(writer).expect("Can't create ArchiveWriter");

                archive.pack(&source).expect("Can't pack");
            },
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, benchmark_single_file_pack_real);
criterion_main!(benches);
