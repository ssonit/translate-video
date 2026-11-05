//! Kiểm tra một file mp4 đã ghi đủ chưa. Tải dở để lại file cụt có đuôi .mp4 mà ffmpeg vẫn đọc được vài
//! giây đầu, nên chỉ nhìn kích thước hay thử mở là không đủ.
//!
//! Luật giống `mp4_complete` trong src-tauri/douyin/fetch.py (sửa bên này thì sửa cả bên kia): các hộp cấp
//! cao nối khít tới cuối file, và có cả moov lẫn mdat.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub(super) fn mp4_complete(path: &Path) -> bool {
    walk(path).unwrap_or(false)
}

// Chỉ đọc đầu mỗi hộp (tối đa 16 byte), không đọc phần ruột nên file vài GB cũng kiểm tra tức thì.
fn walk(path: &Path) -> std::io::Result<bool> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let (mut moov, mut mdat) = (false, false);
    let mut offset = 0u64;
    while offset < size {
        file.seek(SeekFrom::Start(offset))?;
        let mut head = [0u8; 16];
        let mut have = 0;
        while have < head.len() {
            let read = file.read(&mut head[have..])?;
            if read == 0 {
                break;
            }
            have += read;
        }
        if have < 8 {
            return Ok(false);
        }
        let mut length = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let mut header = 8;
        match length {
            1 => {
                if have < 16 {
                    return Ok(false);
                }
                length = u64::from_be_bytes([
                    head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
                ]);
                header = 16;
            }
            0 => length = size - offset,
            _ => {}
        }
        if length < header || length > size - offset {
            return Ok(false);
        }
        match &head[4..8] {
            b"moov" => moov = true,
            b"mdat" => mdat = true,
            _ => {}
        }
        offset += length;
    }
    Ok(offset == size && moov && mdat)
}

// Một file mp4 đủ hộp cho test ở các module cùng cấp.
#[cfg(test)]
pub(super) fn sample_mp4(mdat_size: usize) -> Vec<u8> {
    tests::whole(mdat_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mp4-complete-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = ((8 + payload.len()) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn ftyp() -> Vec<u8> {
        boxed(b"ftyp", b"isom\0\0\x02\0isomiso2")
    }

    fn moov() -> Vec<u8> {
        boxed(b"moov", &[0; 100])
    }

    fn mdat(size: usize) -> Vec<u8> {
        boxed(b"mdat", &vec![0x11; size])
    }

    pub(super) fn whole(mdat_size: usize) -> Vec<u8> {
        [ftyp(), moov(), mdat(mdat_size)].concat()
    }

    fn check(name: &str, bytes: &[u8]) -> bool {
        let dir = scratch(name);
        let path = dir.join("video.mp4");
        fs::write(&path, bytes).unwrap();
        let result = mp4_complete(&path);
        let _ = fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn a_file_with_every_box_in_place_is_complete() {
        assert!(check("whole", &whole(300_000)));
    }

    #[test]
    fn moov_after_mdat_is_still_complete() {
        let bytes = [ftyp(), mdat(300_000), moov()].concat();
        assert!(check("moov-last", &bytes));
    }

    #[test]
    fn a_file_cut_in_the_middle_of_mdat_is_not_complete() {
        let bytes = [ftyp(), moov(), mdat(300_000)].concat();
        assert!(!check("cut-mdat", &bytes[..bytes.len() - 1]));
        assert!(!check("cut-half", &bytes[..bytes.len() / 2]));
    }

    #[test]
    fn a_file_without_moov_is_not_complete() {
        let bytes = [ftyp(), mdat(300_000)].concat();
        assert!(!check("no-moov", &bytes));
    }

    #[test]
    fn a_file_without_mdat_is_not_complete() {
        let bytes = [ftyp(), moov()].concat();
        assert!(!check("no-mdat", &bytes));
    }

    #[test]
    fn an_empty_file_is_not_complete() {
        assert!(!check("empty", &[]));
    }

    #[test]
    fn bytes_after_the_last_box_that_do_not_form_a_box_are_not_complete() {
        let mut bytes = [ftyp(), moov(), mdat(1_000)].concat();
        bytes.extend_from_slice(&[0, 0, 0]);
        assert!(!check("trailing", &bytes));
    }

    #[test]
    fn a_box_may_use_a_64_bit_size() {
        let payload = vec![0x22u8; 5_000];
        let mut big = 1u32.to_be_bytes().to_vec();
        big.extend_from_slice(b"mdat");
        big.extend_from_slice(&((16 + payload.len()) as u64).to_be_bytes());
        big.extend_from_slice(&payload);
        assert!(check("large-size", &[ftyp(), moov(), big.clone()].concat()));
        assert!(!check(
            "large-size-cut",
            &[ftyp(), moov(), big[..big.len() - 1].to_vec()].concat()
        ));
    }

    #[test]
    fn a_box_claiming_an_absurd_size_is_not_complete_and_does_not_overflow() {
        // File hỏng khai size 64-bit gần u64::MAX: cộng thẳng vào vị trí sẽ tràn số.
        let mut huge = 1u32.to_be_bytes().to_vec();
        huge.extend_from_slice(b"mdat");
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        huge.extend_from_slice(&[0x44; 100]);
        assert!(!check("absurd-size", &[ftyp(), moov(), huge].concat()));
    }

    #[test]
    fn a_last_box_of_size_zero_runs_to_the_end_of_the_file() {
        let mut open = 0u32.to_be_bytes().to_vec();
        open.extend_from_slice(b"mdat");
        open.extend_from_slice(&[0x33; 4_000]);
        assert!(check("to-eof", &[ftyp(), moov(), open].concat()));
    }

    #[test]
    fn a_box_smaller_than_its_own_header_is_not_complete() {
        // Hộp khai báo dài 4 byte, ngắn hơn cả phần đầu 8 byte. Nếu cứ nhảy 4 byte thì "hộp kế tiếp" nằm chồng
        // lên chỗ kiểu hộp và trông hợp lệ, file bị tính là đủ.
        let mut broken = 4u32.to_be_bytes().to_vec();
        broken.extend_from_slice(&8u32.to_be_bytes());
        broken.extend_from_slice(b"free");
        let bytes = [ftyp(), moov(), mdat(1_000), broken].concat();
        assert!(!check("tiny-box", &bytes));
    }

    #[test]
    fn a_missing_file_is_not_complete() {
        assert!(!mp4_complete(
            &std::env::temp_dir().join("mp4-complete-no-such-file.mp4")
        ));
    }
}
