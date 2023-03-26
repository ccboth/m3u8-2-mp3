#![allow(unused_parens)]

use std::fs::{self};
use crypto::{ symmetriccipher, buffer, aes };
use crypto::buffer::{ ReadBuffer, WriteBuffer, BufferResult };

#[tokio::main]
#[inline]
async fn main() {
    const M3U8: &str = "YOUR LINK";
    const RESULT_OUT: &str = "../../out.mp3";
    let mut track = match M3u8ToMp3Convertor::from_url(M3U8).await {
        Ok(conv) => conv,
        Err(_err) => panic!("Err with create obj"),
    };
    // let playlist = obey.get_m3u8();
    let mp3 = match track.load().await {
        Ok(res) => res,
        Err(_err) => panic!("error with load mp3")
    };
    println!("{:?}", mp3.len());

    match fs::write(RESULT_OUT, mp3) {
        Ok(()) => println!("all success"),
        Err(err) => panic!("{:?}", err)
    };

    track.set_relative_path_for_tracks("");
}

pub enum M3u8ToMp3ConvertionErrors {
    RequestError,
    PlaylistTypeIsMasterError,
    ParseM3u8PlaylistError,
    DecryptionError,
    UnsupportMpegType,
}

pub struct M3u8ToMp3Convertor {
    relative_playlist_path: String,
    playlist: m3u8_rs::MediaPlaylist,
}


impl M3u8ToMp3Convertor {

    
    
    /// This good method can convert .m3u8 file with just a link? Yes.
    /// 
    /// Arguments:
    /// 
    /// * `url`: The url of the m3u8 file.
    /// 
    /// Returns:
    /// 
    /// A Result<Self, reqwest::Error>
    pub async fn from_url(url: &str) -> Result<Self, M3u8ToMp3ConvertionErrors> {
        let mut binary_data_m3u8: Vec<u8>;   
        let mut convertor: Self; 
        let url = url.to_string();
        let base_name = url
            .split("/")
            .last()
            .unwrap()
            .to_string();
        let relative_path = url.replace(&base_name, "");

        match request_bytes(&url).await {
            Ok(response_bytes) => binary_data_m3u8 = response_bytes,
            Err(_err) => return Err(M3u8ToMp3ConvertionErrors::RequestError)
        }

        match Self::from_byte_array(&mut binary_data_m3u8) {
            Ok(cnvrtr) => convertor = cnvrtr,
            Err(err) => return Err(err)
        };

        convertor.set_relative_path_for_tracks(relative_path.as_str());

        return Ok(convertor);
    }


    /// It parses the playlist and returns a `Playlist` struct.
    /// 
    /// Arguments:
    /// 
    /// * `byte_array`: The byte array of the playlist file.
    /// 
    /// Returns:
    /// 
    /// A ready to load playlist
    fn from_byte_array(byte_array: &Vec<u8>) -> Result<Self, M3u8ToMp3ConvertionErrors> {
        let playlist = match m3u8_rs::parse_playlist(byte_array) {
            Result::Ok((_i, m3u8_rs::Playlist::MediaPlaylist(pl))) => pl,
            Result::Ok((_i, m3u8_rs::Playlist::MasterPlaylist(_pl))) => return Err(M3u8ToMp3ConvertionErrors::PlaylistTypeIsMasterError),
            Result::Err(_err) => return Err(M3u8ToMp3ConvertionErrors::ParseM3u8PlaylistError)
        };

        return Ok(Self {
            relative_playlist_path: "".to_string(),
            playlist
        })
    }
    

    /// It downloads the playlist, then downloads each segment, decrypts it, and appends it to the
    /// output file
    /// 
    /// Returns:
    /// 
    /// A vector of bytes.
    pub async fn load(&self) -> Result<Vec<u8>, M3u8ToMp3ConvertionErrors> {
        let mut out_file: Vec<u8> = Vec::new();
        let mut is_first = true;

        for segment in self.playlist.segments.iter() {
            let mut iv: [u8; 16] = [0; 16]; iv[15] = 1;
            let key_response:Vec<u8>;
            let url = format!("{}/{}", self.relative_playlist_path, segment.uri);
            let mut raw = match request_bytes(&url).await {
                Ok(raw) => raw,
                Err(_err) => return Err(M3u8ToMp3ConvertionErrors::RequestError)
            };
            
            let key = segment.key.clone();
            if (key == None) {
                out_file.append(&mut raw);
                continue;
            }
            // continue;
            key_response = match request_bytes(&key.unwrap().uri.unwrap()).await {
                Ok(resp) => resp,
                Err(_err) => return Err(M3u8ToMp3ConvertionErrors::RequestError)
            };
            
            raw = match decrypt(&mut raw, &key_response, &iv) {
                Ok(res) => res,
                Err(_er) => return Err(M3u8ToMp3ConvertionErrors::DecryptionError)
            };

            if (!is_first) {
                raw = match exclude_header_data(raw) {
                    Ok(res) => res,
                    Err(er) => return Err(er)
                };
                
            } else { is_first = false; }

            out_file.append(&mut raw);
            std::thread::sleep(std::time::Duration::new(1, 0));
            // println!("Ended {}", self.playlist.segments.len());
            // exclude_header_data(&mut raw);
            // let decrypted = Aes128::new(&key_16_byte);
            // break;
        }

        return Ok(out_file);
    }

    fn set_relative_path_for_tracks(&mut self, path: &str) -> &Self {
        self.relative_playlist_path = path.to_string();
        return self;
    }
}


async fn request_bytes(url: &str) -> Result<Vec<u8>, reqwest::Error> {

    let resp = reqwest::get(url).await;
    return match resp {
        Ok(rsp) => Ok(rsp.bytes().await.unwrap().to_vec()),
        Err(failed_resp) => Err(failed_resp)
    };
    
}


fn decrypt(encrypted_data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, symmetriccipher::SymmetricCipherError> {
    let mut decryptor = aes::cbc_decryptor(
            aes::KeySize::KeySize128,
            key,
            iv,
            crypto::blockmodes::PkcsPadding);

    // keeps track of how much data has been written or read
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(encrypted_data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    // loops through and slices the data, allowing it to continue to make decryption passes
    loop {
        let result = r#try!(decryptor.decrypt(&mut read_buffer, &mut write_buffer, true));
        final_result.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => { }
        }
    }

    return Ok(final_result);
}


fn exclude_header_data(data: Vec<u8>) -> Result<Vec<u8>, M3u8ToMp3ConvertionErrors> {
    // There must be some algorithm for removing headers here
    // But I don't have time to finish it.
    // Someday I'll make an adequate MPEG parser, and get back to work.
    
    if data.len() < 184 {
        return Err(M3u8ToMp3ConvertionErrors::UnsupportMpegType);
    }

    return Ok(data[184..].to_vec());
}