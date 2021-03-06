//! PSOPC and BB patch server structures.

#[macro_use] extern crate psoserial;
extern crate typenum;
extern crate byteorder;
#[macro_use] extern crate log;
extern crate staticvec;
extern crate psomsg_common;

use typenum::consts::{U12, U16, U32, U48, U64};

use psoserial::Serial;
use staticvec::StaticVec;
use psomsg_common::util::*;

use std::io::{Read, Write};
use std::io;
use std::net::{SocketAddrV4, Ipv4Addr};

use byteorder::{LittleEndian as LE, BigEndian as BE, ReadBytesExt, WriteBytesExt};

macro_rules! gen_message_enum_patch {
    ($($id:expr => $name:ident),*) => {
        #[derive(Clone, Debug)]
        pub enum Message {
            Unknown(u16, Option<Vec<u8>>),
            $($name(Option<$name>)),*
        }

        impl Serial for Message {
            fn serialize(&self, dst: &mut Write) -> io::Result<()> {
                use std::io::Cursor;
                let mut buf = Vec::with_capacity(256);
                let msg_type: u16;
                let mut size: u16;

                {
                    let mut cur = Cursor::new(buf);
                    match self {
                        &Message::Unknown(ref a, Some(ref bytes)) => {
                            try!(cur.write_all(&bytes[..]));
                            msg_type = *a;
                        },
                        &Message::Unknown(ref a, None) => {
                            msg_type = *a;
                        }
                        $(&Message::$name(Some(ref a)) => {
                            try!(a.serialize(&mut cur));
                            msg_type = $id as u16;
                        },)*
                        $(&Message::$name(None) => {
                            msg_type = $id as u16;
                        }),*
                    }
                    size = cur.position() as u16;
                    buf = cur.into_inner();
                }

                debug!("serializing msg_type 0x{:x}, size {}", msg_type, size);

                // do not ever pad the message
                if buf.len() % 4 != 0 {
                    let buf_len = buf.len();
                    buf.append(&mut vec![0u8; (4 - buf_len % 4) as usize]);
                    size += (4 - buf_len % 4) as u16;
                }
                let hdr_buf;
                {
                    let mut curs = Cursor::new(Vec::with_capacity(4));
                    try!(curs.write_u16::<LE>(size + 4));
                    try!(curs.write_u16::<LE>(msg_type));
                    hdr_buf = curs.into_inner();
                }
                try!(dst.write_all(&hdr_buf));
                try!(dst.write_all(&buf));
                Ok(())
            }

            fn deserialize(src: &mut Read) -> io::Result<Self> {
                use std::io::Cursor;
                let mut hdr_buf = vec![0u8; 4];
                debug!("Reading patch header");
                try!(read_exact(src, &mut hdr_buf[..]));
                let size;
                let msg_type;
                {
                    debug!("Parsing patch header");
                    let mut hdr_curs = Cursor::new(hdr_buf);
                    size = try!(hdr_curs.read_u16::<LE>());
                    msg_type = try!(hdr_curs.read_u16::<LE>());
                }
                debug!("size: {}, type: 0x{:x}", size, msg_type);

                let padding = if size % 4 == 0 { 0 } else { 4 - (size % 4) };
                let mut msg_buf = vec![0u8; (size + padding) as usize - 4];
                if size > 4 {
                    debug!("Reading patch message contents");
                    try!(read_exact(src, &mut msg_buf));
                    let mut msg_cur = Cursor::new(msg_buf);
                    match msg_type {
                        $($id => Ok(Message::$name(Some(try!($name::deserialize(&mut msg_cur))))),)*
                        a => {
                            Ok(Message::Unknown(a, Some(msg_cur.into_inner())))
                        }
                    }
                } else {
                    debug!("Patch Message was header only");
                    // header only
                    match msg_type {
                        $($id => Ok(Message::$name(None)),)*
                        a => {
                            Ok(Message::Unknown(a, None))
                        }
                    }
                }
            }
        }

        $(
            impl From<$name> for Message {
                #[inline(always)]
                fn from(val: $name) -> Message {
                    Message::$name(Some(val))
                }
            }

            impl From<Option<$name>> for Message {
                #[inline(always)]
                fn from(val: Option<$name>) -> Message {
                    Message::$name(val)
                }
            }
        )*
    }
}

gen_message_enum_patch! {
    0x02 => Welcome,
    0x04 => Login,
    0x06 => FileSend,
    0x07 => DataSend,
    0x08 => FileDone,
    0x09 => SetDirectory,
    0x0A => OneDirUp,
    0x0B => StartList,
    0x0C => FileInfo,
    0x0D => InfoFinished,
    0x0F => FileInfoReply,
    0x10 => FileListDone,
    0x11 => SendInfo,
    0x12 => SendDone,
    0x13 => Motd,
    0x14 => Redirect,
    0x614 => Redirect6
}

#[derive(Clone, Debug)]
pub struct Welcome {
    pub server_vector: u32,
    pub client_vector: u32
}

impl Serial for Welcome {
    fn serialize(&self, dst: &mut Write) -> io::Result<()> {
        try!(dst.write_all(b"Patch Server. Copyright SonicTeam, LTD. 2001"));
        try!(dst.write_all(&[0; 20]));
        try!(dst.write_u32::<LE>(self.server_vector));
        try!(dst.write_u32::<LE>(self.client_vector));
        Ok(())
    }

    fn deserialize(src: &mut Read) -> io::Result<Self> {
        if try!(src.read(&mut [0; 64])) != 64 {
            return Err(io::Error::new(io::ErrorKind::Other, "Unexpected EOF"))
        }
        Ok(Welcome {
            server_vector: try!(src.read_u32::<LE>()),
            client_vector: try!(src.read_u32::<LE>())
        })
    }
}

derive_serial!(Login {
    pub padding1: StaticVec<u8, U12>,
    pub username: StaticVec<u8, U16>,
    pub password: StaticVec<u8, U16>,
    pub padding2: StaticVec<u8, U64>
});

derive_serial!(FileSend {
    pub padding: u32,
    pub size: u32,
    pub filename: StaticVec<u8, U48>
});

derive_serial!(DataSend {
    pub chunk_num: u32,
    pub checksum: u32,
    pub chunk_size: u32
});

derive_serial!(FileDone { pub padding: u32 });
derive_serial!(SetDirectory { pub dirname: StaticVec<u8, U64> });
derive_serial!(OneDirUp);
derive_serial!(StartList);
derive_serial!(FileInfo { pub patch_id: u32, pub filename: StaticVec<u8, U32> });
derive_serial!(InfoFinished);
derive_serial!(FileInfoReply { pub patch_id: u32, pub checksum: u32, pub size: u32 });
derive_serial!(FileListDone);
derive_serial!(SendInfo { pub total_length: u32, pub total_file: u32 });
derive_serial!(SendDone);

#[derive(Clone, Debug)]
pub struct Motd {
    pub message: String
}
impl Serial for Motd {
    fn serialize(&self, dst: &mut Write) -> io::Result<()> {
        try!(write_utf16(&self.message, dst));
        Ok(())
    }

    fn deserialize(src: &mut Read) -> io::Result<Self> {
        let message = try!(read_utf16(src));
        Ok(Motd {
            message: message
        })
    }
}

#[derive(Clone, Debug)]
pub struct Redirect(pub SocketAddrV4);
impl Serial for Redirect {
    fn serialize(&self, dst: &mut Write) -> io::Result<()> {
        let ip_addr: u32 = self.0.ip().clone().into();
        let port: u16 = self.0.port();
        try!(dst.write_u32::<BE>(ip_addr));
        try!(dst.write_u16::<BE>(port));
        try!(dst.write_u16::<LE>(0));
        Ok(())
    }

    fn deserialize(src: &mut Read) -> io::Result<Self> {
        let ip_addr: Ipv4Addr = try!(src.read_u32::<BE>()).into();
        let port: u16 = try!(src.read_u16::<BE>());
        Ok(Redirect(SocketAddrV4::new(ip_addr, port)))
    }
}

derive_serial!(Redirect6 { pub ip_addr: StaticVec<u8, U16>, pub port: u16, pub padding: u16 });
