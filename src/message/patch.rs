use ::message::prelude::*;
use std::io::Cursor;

/// Struct format for patch header: u16 len, u16 msg_type
pub struct HdrSerializer;

impl HdrSerial for HdrSerializer {
    fn hdr_deserialize(src: &mut Read, decryptor: Option<&mut Decryptor>) -> io::Result<MsgHeader> {
        let len;
        let msg_type;

        if let Some(d) = decryptor {
            let mut rbuf = [0u8; 4];
            let mut ebuf = [0u8; 4];
            {
                if try!(src.read(&mut rbuf)) != 4 {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe reading header"))
                }
            }
            {
                if let Err(e) = d.decrypt(&mut RefReadBuffer::new(&rbuf), &mut RefWriteBuffer::new(&mut ebuf), false) {
                    return Err(io::Error::new(io::ErrorKind::Other, format!("Encryption failed: {:?}", e)))
                }
            }
            {
                let mut cur = Cursor::new(&ebuf);
                len = try!(cur.read_u16::<LittleEndian>());
                msg_type = try!(cur.read_u16::<LittleEndian>());
            }
        } else {
            len = try!(src.read_u16::<LittleEndian>());
            msg_type = try!(src.read_u16::<LittleEndian>());
        }

        Ok(MsgHeader {
            len: len as u32 - 4,
            msg_type: msg_type as u32,
            flags: 0
        })
    }

    fn hdr_serialize(value: &MsgHeader, dst: &mut Write, encryptor: Option<&mut Encryptor>) -> io::Result<()> {
        let MsgHeader { len, msg_type, .. } = *value;
        if let Some(e) = encryptor {
            let mut wbuf = [0u8; 4];
            let mut ebuf = [0u8; 4];
            {
                let mut cur = Cursor::new(&mut wbuf[..]);
                try!(cur.write_u16::<LittleEndian>(len as u16 + 4));
                try!(cur.write_u16::<LittleEndian>(msg_type as u16));
            }
            {
                if let Err(_) = e.encrypt(&mut RefReadBuffer::new(&wbuf),
                    &mut RefWriteBuffer::new(&mut ebuf[..]), false) {
                    return Err(io::Error::new(io::ErrorKind::Other, "encryption failed at header"))
                }
            }
            dst.write_all(&ebuf)
        } else {
            try!(dst.write_u16::<LittleEndian>(len as u16 + 4));
            try!(dst.write_u16::<LittleEndian>(msg_type as u16));
            Ok(())
        }
    }
}

define_messages! {
    0x02 => Welcome { server_vector: u32, client_vector: u32;
        impl Serial for Welcome {
            fn serial_len(_: &Welcome) -> usize { 72 }
            fn serialize(value: &Welcome, dst: &mut Write) -> io::Result<()> {
                try!(dst.write_all(b"Patch Server. Copyright SonicTeam, LTD. 2001"));
                try!(dst.write_all(&[0u8; 20]));
                try!(dst.write_u32::<LittleEndian>(value.server_vector));
                try!(dst.write_u32::<LittleEndian>(value.client_vector));
                // try!(<u32 as Serial>::serialize(&value.server_vector, dst));
                // try!(<u32 as Serial>::serialize(&value.client_vector, dst));
                Ok(())
            }
            fn deserialize(src: &mut Read) -> io::Result<Welcome> {
                let empty_bytes = try!(src.read(&mut [0u8; 64]));
                if empty_bytes != 64 {
                    return Err(io::Error::new(io::ErrorKind::Other, format!("expected 64 bytes, got {}", empty_bytes)));
                }
                Ok(Welcome {
                    server_vector: try!(src.read_u32::<LittleEndian>()),
                    client_vector: try!(src.read_u32::<LittleEndian>())
                })
            }
        }
    },
    0x04 => Login { username: [u8; 16], password: [u8; 16];
        impl Serial for Login {
            fn serial_len(_: &Login) -> usize { 108 }
            fn serialize(value: &Login, dst: &mut Write) -> io::Result<()> {
                try!(dst.write_all(&[0u8; 12]));
                try!(dst.write_all(&value.username));
                try!(dst.write_all(&value.password));
                try!(dst.write_all(&[0u8; 64]));
                Ok(())
            }
            fn deserialize(src: &mut Read) -> io::Result<Login> {
                let mut username: [u8; 16] = [0; 16];
                let mut password: [u8; 16] = [0; 16];
                try!(src.read(&mut [0u8; 12]));
                try!(src.read(&mut username));
                try!(src.read(&mut password));
                try!(src.read(&mut [0u8; 64]));
                Ok(Login {
                    username: username,
                    password: password
                })
            }
        }
    },
    0x06 => FileSend { size: u32, filename: Vec<u8>;
        impl Serial for FileSend {
            fn serial_len(_: &FileSend) -> usize { 8 + 48 }
            fn serialize(value: &FileSend, dst: &mut Write) -> io::Result<()> {
                let mut filename_buf = [0u8; 48];
                if value.filename.len() > 48 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("filename is too long, expected 48 got {}",
                            value.filename.len())));
                }
                for (i, x) in value.filename.iter().enumerate() {
                    filename_buf[i] = *x;
                }
                try!(dst.write_u32::<LittleEndian>(0)); // padding
                try!(dst.write_u32::<LittleEndian>(value.size));
                try!(dst.write_all(&filename_buf));
                Ok(())
            }
            fn deserialize(src: &mut Read) -> io::Result<FileSend> {
                try!(src.read_u32::<LittleEndian>());
                Ok(FileSend {
                    size: try!(src.read_u32::<LittleEndian>()),
                    filename: {
                        let mut buf = vec![0u8; 48];
                        if try!(src.read(&mut buf)) != 48 {
                            return Err(io::Error::new(io::ErrorKind::Other, "Unexpected EOF"));
                        }
                        buf.into()
                    }
                })
            }
        }
    },
    0x07 => DataSend { chunk_num: u32, checksum: u32, chunk_size: u32 },
    0x08 => FileDone { padding: u32 },
    0x09 => SetDirectory { dirname: Vec<u8>;
        impl Serial for SetDirectory {
            fn serial_len(_: &SetDirectory) -> usize { 64 }
            fn serialize(value: &SetDirectory, dst: &mut Write) -> io::Result<()> {
                let mut dirname_buf = [0u8; 64];
                for (i, x) in value.dirname.iter().enumerate() {
                    dirname_buf[i] = *x
                }
                try!(dst.write_all(&dirname_buf));
                Ok(())
            }
            fn deserialize(src: &mut Read) -> io::Result<SetDirectory> {
                Ok(SetDirectory {
                    dirname: {
                        let mut buf = [0u8; 64];
                        let mut r = Vec::with_capacity(64);
                        if try!(src.read(&mut buf)) != 64 {
                            return Err(io::Error::new(io::ErrorKind::Other, "Unexpected EOF"))
                        }
                        for x in buf.iter() { r.push(*x) }
                        r
                    }
                })
            }
        }
    },
    0x0A => OneDirUp {},
    0x0B => StartList {},
    0x0C => FileInfo { patch_id: u32, filename: Vec<u8>;
        impl Serial for FileInfo {
            fn serial_len(_: &FileInfo) -> usize { 4 + 32 }

            fn serialize(value: &FileInfo, dst: &mut Write) -> io::Result<()> {
                let mut filename_buf = [0u8; 32];
                for (i, x) in value.filename.iter().enumerate() {
                    filename_buf[i] = *x;
                }
                try!(dst.write_u32::<LittleEndian>(value.patch_id));
                try!(dst.write_all(&filename_buf));
                Ok(())
            }

            fn deserialize(src: &mut Read) -> io::Result<FileInfo> {
                Ok(FileInfo {
                    patch_id: try!(src.read_u32::<LittleEndian>()),
                    filename: {
                        let mut buf = [0u8; 32];
                        let mut r = Vec::with_capacity(32);
                        if try!(src.read(&mut buf)) != 32 {
                            return Err(io::Error::new(io::ErrorKind::Other, "Unexpected EOF"))
                        }
                        for x in buf.iter() { r.push(*x) }
                        r
                    }
                })
            }
        }
    },
    0x0D => InfoFinished {},
    0x0F => FileInfoReply { patch_id: u32, checksum: u32, size: u32 },
    0x10 => FileListDone {},
    0x11 => SendInfo { total_length: u32, total_files: u32 },
    0x12 => SendDone {},
    0x13 => Motd { message: String;
        impl Serial for Motd {
            fn serial_len(value: &Motd) -> usize {
                use encoding::{Encoding, EncoderTrap};
                use encoding::all::UTF_16LE;
                let enc = match UTF_16LE.encode(&value.message, EncoderTrap::Replace) {
                    Ok(a) => a,
                    Err(_) => panic!("oh so uh this Motd string is not UTF-16 encodable")
                };
                // must be divisible by 2 for encryption reasons
                if enc.len() % 2 > 0 {
                    enc.len() + 1
                } else { enc.len() }
            }

            fn serialize(value: &Motd, dst: &mut Write) -> io::Result<()> {
                use encoding::{Encoding, EncoderTrap};
                use encoding::all::UTF_16LE;
                use std::borrow::Borrow;
                let enc = match UTF_16LE.encode(&value.message, EncoderTrap::Replace) {
                    Ok(a) => a,
                    Err(_) => return Err(io::Error::new(io::ErrorKind::Other,
                        "Failed to encode Motd into UTF-16"))
                };
                try!(dst.write_all(enc.borrow()));
                Ok(())
            }

            fn deserialize(_: &mut Read) -> io::Result<Motd> {
                unimplemented!();
            }
        }
    },
    0x14 => Redirect { ip_addr: u32, port: u16, padding: u16 },
    0x614 => Redirect6 { ip_addr: [u8; 16], port: u16, padding: u16 }
}

#[cfg(test)]
mod tests {
    use super::*;
    test_size!(Welcome, size_welcome, 0x4c - 4);
    test_size!(Redirect, size_redirect, 0xc - 4);
    test_size!(Redirect6, size_redirect6, 0x18 - 4);
    test_size!(FileSend, size_filesend, 0x3c - 4);
    test_size!(DataSend, size_datasend, 0x10 - 4);
    test_size!(FileDone, size_filedone, 0x8 - 4);
    test_size!(SetDirectory, size_setdir, 0x44 - 4);
    test_size!(FileInfo, size_fileinfo, 0x28 - 4);
    test_size!(FileInfoReply, size_fileinforeply, 0x10 - 4);
    test_size!(SendInfo, size_sendinfo, 0xc - 4);

    test_size!(StartList, size_startlist, 0);
    test_size!(OneDirUp, size_onedirup, 0);
    test_size!(FileListDone, size_filelistdone, 0);
    test_size!(SendDone, size_senddone, 0);
    test_size!(InfoFinished, size_infofinished, 0);
    }
