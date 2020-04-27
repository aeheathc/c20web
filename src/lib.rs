#[macro_use]
extern crate lazy_static;

use std::io::{ErrorKind};
use std::fs;
use std::io::prelude::*;
use std::net::Shutdown;
use std::net::TcpStream;
use std::path::Path;

use log::{error, warn, /*info, debug,*/ trace, log, Level};

pub mod statics;
use statics::SETTINGS;
use statics::HTTP_RESPONSE_TABLE;
use statics::MIME_BY_EXTENSION;


pub struct Request
{
	method: String,
	resource: String,
	http_version: String,
}

impl Request
{
	pub fn parse(buffer: Box<[u8]>) -> Result<Request,Response>
	{
		//find the necessary parts in the request
		let mut index_end_method = 0;
		let mut index_end_resource = 0;
		let mut index_end_line = 0;
		for (index, request_byte) in buffer.iter().enumerate()
		{
			if *request_byte == b'\r' || *request_byte == b'\n'
			{
				index_end_line = index;
				break;
			}else if *request_byte == b' '{
				if index_end_method == 0 {index_end_method = index;}
				else if index_end_resource == 0 {index_end_resource = index;}
			}
		}

		if index_end_line == 0 || index_end_resource == 0 || index_end_method == 0
		{
			Err(Response::new(400, String::from("Malformed request line")))
		}else{
			let method: &str = match std::str::from_utf8(&(buffer[0..index_end_method]))
			{
				Ok(s) => s,
				Err(e) => {return Err(Response::new(400, format!("Malformed method name: {}",e)));}
			};
			let resource: &str = match std::str::from_utf8(&(buffer[(index_end_method+1)..index_end_resource]))
			{
				Ok(s) => s,
				Err(e) => {return Err(Response::new(400, format!("Malformed resource name: {}",e)));}
			};
			let http_version: &str = match std::str::from_utf8(&(buffer[(index_end_resource+1)..index_end_line]))
			{
				Ok(s) => s,
				Err(e) => {return Err(Response::new(400, format!("Malformed http version: {}",e)));}
			};

			Ok(Request{method: String::from(method), resource: String::from(resource), http_version: String::from(http_version)})
		}
	}
}

pub enum Data
{
	Binary(Vec::<u8>),
	Text(String)
}

pub struct Response
{
	code: u16,
	mime: String,
	body: Data
}

impl Response
{
	pub fn new(code: u16, body: String) -> Response
	{
		Response{code: code, mime:String::from("text/html"), body: Data::Text(body)}
	}

	pub fn to_vec(&self) -> Vec::<u8>
	{
		let status = if let Some(status_str) = HTTP_RESPONSE_TABLE.get(&self.code)
		{
			format!("{} {}",self.code,status_str)
		}else{
			warn!("Returning HTTP response code with no name: {}", self.code);
			format!("{} Unknown",self.code)
		};
	
		let mut body_out: Vec::<u8> = if self.code < 200 || self.code >= 300
		{
			let mut error_page = match fs::read_to_string("error.html")
			{
				Err(e) => {
					warn!("Using default error page because we wouldn't find error.html - {}",e);
					String::from("<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'><title>{}</title></head><body><h1>{}</h1><p>{}</p></body></html>")
				},
				Ok(body) => body
			};
			error_page = error_page.replacen("{}", &status, 2);
			let error_descr = if let Data::Text(t) = &self.body{t.clone()}else{String::from("")};
			error_page.replacen("{}", &error_descr, 1).as_bytes().to_vec()
		}else{
			match &self.body{
				Data::Binary(b) => b.to_owned(),
				Data::Text(t) => t.as_bytes().to_vec()
			}
		};

		let mut out = (format!("HTTP/1.1 {}\r\nContent-Type: {};\r\nContent-Length: {};\r\n\r\n", status, self.mime, body_out.len())).as_bytes().to_vec();
		out.append(&mut body_out);
		out
	}
}

pub fn handle_connection(mut stream: TcpStream)
{
	trace!("Starting to process request.");
	let settings = match SETTINGS.read(){
		Ok(r) => r,
		Err(e) => {error!("Couldn't get config in request thread: {}",e); return;}
	};
	let webroot = match settings.get::<String>("webroot"){
		Ok(r) => r,
		Err(e) => {error!("webroot missing from config: {}",e); return;}
	};
	let request_max_bytes = match settings.get::<usize>("request_max_bytes"){
		Ok(r) => r,
		Err(e) => {error!("request_max_bytes missing from config: {}",e); return;}
	};

	trace!("Creating buffer");
	let mut buffer = vec![0u8; request_max_bytes+1].into_boxed_slice();
	trace!("Buffer created. Reading input");
	let request_result = stream.read(&mut buffer);

	/* Any output won't make it to the browser if there is still input left to be read.
	 * In order to avoid DoS attacks by enforcing max request size, and still
	 * send the appropriate error message back, we need to discard the rest of
	 * the input without actually reading it in. Even calling shutdown on Read doesn't
	 * always do this but there doesn't seem to be any better way.
	*/
	let _shutdown_res = stream.shutdown(Shutdown::Read);

	trace!("Request read. Starting analysis");
	let response: Response = match request_result
	{
		Ok(num_bytes) => {
			if num_bytes >= request_max_bytes
			{
				Response::new(413, String::from(""))
			}else{
				match Request::parse(buffer)
				{
					Ok(request) => {
						//determine whether we currently support the features necessary to fulfill the request
						if request.method != "GET"
						{
							Response::new(501, String::from("This server only accepts GET requests."))
						}else if request.http_version != "HTTP/1.1"{
							Response::new(505, String::from("This server only speaks HTTP/1.1"))
						}else{
							//attempt to load the requested file
							let resource = request.resource.replacen(&"/",&"",1);
							let resource = format!("{}/{}", webroot, resource);
							trace!("Requesting page: {}",&resource);
							let extension = match Path::new(&resource).extension(){
								Some(x) => match x.to_str(){
										Some(xs) => String::from(xs),
										None => String::from("")
									},
								None => String::from("")
							};
							let mime = if let Some(found_mime) = MIME_BY_EXTENSION.get(&extension)
							{
								found_mime
							}else{
								warn!("Could not find MIME type for file extension: {}", extension);
								"text/plain"
							};
							let body_result = fs::read_to_string(&resource);
							match body_result
							{
								Err(read_err) => {
									if read_err.kind() == ErrorKind::InvalidData
									{
										match std::fs::read(&resource)
										{
											Ok(bytes) => Response{code: 200, mime: String::from(mime), body: Data::Binary(bytes)},
											Err(e) => Response::new(404, format!("{}",e))
										}
									}else{
										Response::new(404, format!("{}",read_err))
									}
								},
								Ok(body) => {
									Response{code: 200, mime: String::from(mime), body: Data::Text(body)}
								}
							}
						}
					},
					Err(res) => res
				}
			}
		},
		Err(err_str) => Response::new(400, format!("The network stream didn't stay valid long enough for the server to read it: {}",err_str))
	};
	trace!("Request analyzed. Starting output.");

	//write to request log
	let peer_ip = match stream.peer_addr()
	{
		Ok(r) => r.to_string(),
		Err(e)=> {warn!("Couldn't get peer IP: {}",e); String::from("Unknown")}
	};
	let request_line = format!("From: {} Response code: {}", peer_ip, response.code);
	log!(target: "requests", Level::Info, "{}", request_line);

	//send otuput
	let write_res = stream.write(&(response.to_vec()));
	match write_res
	{
		Ok(_) => {},
		Err(em) => {error!("Write error: {}",em);}
	}
	
	let flush_res = stream.flush();
	match flush_res
	{
		Ok(_) => {},
		Err(em) => {error!("Flush error: {}",em);}
	}
}