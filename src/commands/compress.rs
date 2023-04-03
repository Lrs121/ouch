use std::{
    io::{self, BufWriter, Cursor, Seek, Write},
    path::{Path, PathBuf},
};

use fs_err as fs;

use crate::{
    archive,
    commands::warn_user_about_loading_zip_in_memory,
    extension::{split_first_compression_format, CompressionFormat::*, Extension},
    utils::{user_wants_to_continue, FileVisibilityPolicy},
    QuestionAction, QuestionPolicy, BUFFER_CAPACITY,
};

use super::copy_recursively;

/// Compress files into `output_file`.
///
/// # Arguments:
/// - `files`: is the list of paths to be compressed: ["dir/file1.txt", "dir/file2.txt"]
/// - `extensions`: is a list of compression formats for compressing, example: [Tar, Gz] (in compression order)
/// - `output_file` is the resulting compressed file name, example: "archive.tar.gz"
///
/// # Return value
/// - Returns `Ok(true)` if compressed all files normally.
/// - Returns `Ok(false)` if user opted to abort compression mid-way.
#[allow(clippy::too_many_arguments)]
pub fn compress_files(
    files: Vec<PathBuf>,
    extensions: Vec<Extension>,
    output_file: fs::File,
    output_path: &Path,
    quiet: bool,
    question_policy: QuestionPolicy,
    file_visibility_policy: FileVisibilityPolicy,
    level: Option<i16>,
) -> crate::Result<bool> {
    // If the input files contain a directory, then the total size will be underestimated
    let file_writer = BufWriter::with_capacity(BUFFER_CAPACITY, output_file);

    let mut writer: Box<dyn Send + Write> = Box::new(file_writer);

    // Grab previous encoder and wrap it inside of a new one
    let chain_writer_encoder = |format: &_, encoder| -> crate::Result<_> {
        let encoder: Box<dyn Send + Write> = match format {
            Gzip => Box::new(
                // by default, ParCompress uses a default compression level of 3
                // instead of the regular default that flate2 uses
                gzp::par::compress::ParCompress::<gzp::deflate::Gzip>::builder()
                    .compression_level(
                        level.map_or_else(Default::default, |l| gzp::Compression::new((l as u32).clamp(0, 9))),
                    )
                    .from_writer(encoder),
            ),
            Bzip => Box::new(bzip2::write::BzEncoder::new(
                encoder,
                level.map_or_else(Default::default, |l| bzip2::Compression::new((l as u32).clamp(1, 9))),
            )),
            Lz4 => Box::new(lz4_flex::frame::FrameEncoder::new(encoder).auto_finish()),
            Lzma => Box::new(xz2::write::XzEncoder::new(
                encoder,
                level.map_or(6, |l| (l as u32).clamp(0, 9)),
            )),
            Snappy => Box::new(
                gzp::par::compress::ParCompress::<gzp::snap::Snap>::builder()
                    .compression_level(gzp::par::compress::Compression::new(
                        level.map_or_else(Default::default, |l| (l as u32).clamp(0, 9)),
                    ))
                    .from_writer(encoder),
            ),
            Zstd => {
                let zstd_encoder = zstd::stream::write::Encoder::new(
                    encoder,
                    level.map_or(zstd::DEFAULT_COMPRESSION_LEVEL, |l| {
                        (l as i32).clamp(zstd::zstd_safe::min_c_level(), zstd::zstd_safe::max_c_level())
                    }),
                );
                // Safety:
                //     Encoder::new() can only fail if `level` is invalid, but the level
                //     is `clamp`ed and therefore guaranteed to be valid
                Box::new(zstd_encoder.unwrap().auto_finish())
            }
            Tar | Zip | Rar | SevenZip => unreachable!(),
        };
        Ok(encoder)
    };

    let (first_format, formats) = split_first_compression_format(&extensions);

    for format in formats.iter().rev() {
        writer = chain_writer_encoder(format, writer)?;
    }

    match first_format {
        Gzip | Bzip | Lz4 | Lzma | Snappy | Zstd => {
            writer = chain_writer_encoder(&first_format, writer)?;
            let mut reader = fs::File::open(&files[0]).unwrap();

            io::copy(&mut reader, &mut writer)?;
        }
        Tar => {
            archive::tar::build_archive_from_paths(&files, output_path, &mut writer, file_visibility_policy, quiet)?;
            writer.flush()?;
        }
        Zip => {
            if !formats.is_empty() {
                warn_user_about_loading_zip_in_memory();

                if !user_wants_to_continue(output_path, question_policy, QuestionAction::Compression)? {
                    return Ok(false);
                }
            }

            let mut vec_buffer = Cursor::new(vec![]);

            archive::zip::build_archive_from_paths(
                &files,
                output_path,
                &mut vec_buffer,
                file_visibility_policy,
                quiet,
            )?;
            vec_buffer.rewind()?;
            io::copy(&mut vec_buffer, &mut writer)?;
        },
        Rar => {
            archive::rar::no_compression_notice();
            return Ok(false);
        },
        SevenZip => {
            let tmpdir = tempfile::tempdir()?;

            for filep in files.iter() {
                if filep.is_dir() {
                    copy_recursively(filep, tmpdir.path()
                        .join(filep.strip_prefix(std::env::current_dir()?).expect("copy folder error")))?;
                } else {
                    fs::copy(filep, tmpdir.path().join(filep.file_name().expect("no filename in file")))?;
                }
            }

            sevenz_rust::compress_to_path(tmpdir.path(), output_path).expect("can't compress 7zip archive");
        }
    }

    Ok(true)
}
