use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    time::Duration,
};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use sulfur_archive::{ArchiveReader, ArchiveWriter};
use tempfile::tempdir;

#[allow(clippy::missing_panics_doc)]
pub fn benchmark_pack_multiple_small_files(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("sulfur_bench");

    benchmark_group.sample_size(50);
    benchmark_group.measurement_time(Duration::from_secs(20));

    benchmark_group.bench_function("Multiple files pack (1000 files x 50 KiB)", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().expect("Can't create temp directory");

                let source_dir = dir.path().join("source");
                fs::create_dir(&source_dir).expect("Can't create source directory");

                for i in 0..1000 {
                    let file_path = source_dir.join(format!("file_{i}.txt"));
                    let mut file = File::create(file_path).expect("Can't create file");

                    let pattern = format!("File {i} content");

                    for _ in 0..(50 * 1024 / pattern.len()) {
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

#[allow(clippy::missing_panics_doc)]
pub fn benchmark_unpack_multiple_small_files(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("sulfur_bench");

    benchmark_group.sample_size(50);
    benchmark_group.measurement_time(Duration::from_secs(20));

    benchmark_group.bench_function("Multiple files unpack (1000 files x 50 KiB)", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().expect("Can't create temp directory");

                let source_dir = dir.path().join("source");
                fs::create_dir(&source_dir).expect("Can't create source directory");

                for i in 0..1000 {
                    let file_path = source_dir.join(format!("file_{i}.txt"));
                    let mut file = File::create(file_path).expect("Can't create file");

                    let pattern = format!("File {i} content");

                    for _ in 0..(50 * 1024 / pattern.len()) {
                        file.write_all(pattern.as_bytes())
                            .expect("Can't write into file");
                    }
                    file.sync_all().expect("Can't sync file");
                }

                let archive_path = dir.path().join("test.slf");

                let file = File::create(&archive_path).expect("Can't create archive");
                let writer = BufWriter::new(file);
                let archive = ArchiveWriter::new(writer).expect("Can't create ArchiveWriter");

                archive.pack(&source_dir).expect("Can't pack");

                let extract_dir = dir.path().join("extract");
                fs::create_dir(&extract_dir).expect("Can't create extract directory");

                (dir, archive_path, extract_dir)
            },
            |(_dir, archive_path, extract_dir)| {
                let file = File::open(&archive_path).unwrap();
                let reader = BufReader::new(file);
                let mut archive = ArchiveReader::open(reader).unwrap();
                archive.extract_all(&archive_path, &extract_dir).unwrap();
            },
            BatchSize::LargeInput,
        );
    });
}

#[allow(clippy::missing_panics_doc)]
pub fn benchmark_pack_multiple_large_file(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("sulfur_bench");

    benchmark_group.sample_size(50);
    benchmark_group.measurement_time(Duration::from_secs(20));

    benchmark_group.bench_function("Multiple file pack (10 file x 5 MiB)", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().expect("Can't create temp directory");

                let source_dir = dir.path().join("source");
                fs::create_dir(&source_dir).expect("Can't create source directory");

                for i in 0..10 {
                    let file_path = source_dir.join(format!("file_{i}.txt"));
                    let mut file = File::create(file_path).expect("Can't create file");

                    let pattern = format!("File {i} content");

                    for _ in 0..(5 * 1024 * 1024 / pattern.len()) {
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

#[allow(clippy::missing_panics_doc)]
pub fn benchmark_unpack_multiple_large_file(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("sulfur_bench");

    benchmark_group.sample_size(50);
    benchmark_group.measurement_time(Duration::from_secs(20));

    benchmark_group.bench_function("Multiple file unpack (10 file x 5 MiB)", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().expect("Can't create temp directory");

                let source_dir = dir.path().join("source");
                fs::create_dir(&source_dir).expect("Can't create source directory");

                for i in 0..10 {
                    let file_path = source_dir.join(format!("file_{i}.txt"));
                    let mut file = File::create(file_path).expect("Can't create file");

                    let pattern = format!("File {i} content");

                    for _ in 0..(5 * 1024 * 1024 / pattern.len()) {
                        file.write_all(pattern.as_bytes())
                            .expect("Can't write into file");
                    }
                    file.sync_all().expect("Can't sync file");
                }

                let archive_path = dir.path().join("test.slf");

                let file = File::create(&archive_path).expect("Can't create archive");
                let writer = BufWriter::new(file);
                let archive = ArchiveWriter::new(writer).expect("Can't create ArchiveWriter");

                archive.pack(&source_dir).expect("Can't pack");

                let extract_dir = dir.path().join("extract");
                fs::create_dir(&extract_dir).expect("Can't create extract directory");

                (dir, archive_path, extract_dir)
            },
            |(_dir, archive_path, extract_dir)| {
                let file = File::open(&archive_path).unwrap();
                let reader = BufReader::new(file);
                let mut archive = ArchiveReader::open(reader).unwrap();
                archive.extract_all(&archive_path, &extract_dir).unwrap();
            },
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(
    benches,
    benchmark_pack_multiple_small_files,
    benchmark_unpack_multiple_small_files,
    benchmark_pack_multiple_large_file,
    benchmark_unpack_multiple_large_file
);
criterion_main!(benches);
