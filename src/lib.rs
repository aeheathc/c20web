#[macro_use]
extern crate lazy_static;

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

/**
Represents an HTTP Request.
*/
pub struct Request
{
	pub method: String,
	pub resource: String,
	pub http_version: String,
}

impl Request
{
	/**
	Generates a Request object by parsing the contents of a buffer containing the raw HTTP request data.

	# Parameters
	- `buffer`: byte buffer that the TcpStream wrote into

	# Returns
	Result indicating whether the request is well-formed enough to be parsed
	- `OK`: a Request object containing the important data from the raw request
	- `Err`: a Response object representing the type of error that happened

	# Errors
	Errors produce a Response object with the correct HTTP Response status code for whatever error was encountered.
	The body of the Repsonse will be a string giving additional information, if necessary. Wrap this in the HTML document of your choice.

	# Examples
	```
	use c20web::Response;
	use c20web::Request;

	let buffer = Box::new(b"GET /hello.html HTTP/1.1\r\nUser-Agent: Mozilla/4.0 (compatible; MSIE5.01; Windows NT)\r\nHost: 127.0.0.1:8000\r\n\r\n".to_owned());
	//Determine our response based on what's in the request
	let response: Response = match Request::parse(buffer)
	{
		Ok(request) =>
		{
			//Determine "mime" and "body_content" based on the value of request.resource
			let mime = "text/html";
			let body_content = b"Body Content".to_vec();
			Response{code: 200, mime: String::from(mime), body: body_content}
		},
		Err(res) => res
	};
	```
	*/
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

/**
Represents an HTTP Response.
*/
pub struct Response
{
	pub code: u16,
	pub mime: String,
	pub body: Vec::<u8>
}

impl Response
{
	/**
	Generates a Response object with a default MIME type of text/html.

	# Parameters
	- `code`: HTTP Status code
	- `body`: Contents of the response Body

	# Returns
	Response object, same as if you had manually constructed the object but with a default MIME type.

	# Examples
	```
	use c20web::Response;

	let method_name = "GE7"; //assume we parsed this from the request and found it to not be a method we support
	let out = Response::new(400, format!("Malformed method name: {}",method_name));

	assert_eq!(out.code, 400);
	assert_eq!(out.mime, String::from("text/html"));
	assert_eq!(out.body, String::from("Malformed method name: GE7").as_bytes().to_vec());
	```
	*/
	pub fn new(code: u16, body: String) -> Response
	{
		Response{code, mime:String::from("text/html"), body: body.as_bytes().to_vec()}
	}

	/**
	# Returns
	The response exported as a complete HTTP Response in bytes, ready to be written to an output stream.

	# Examples
	```no_run
	use c20web::Response;
	use std::net::TcpListener;
	use std::io::Write;

	let listener = TcpListener::bind("127.0.0.1:8000").unwrap();
    for mut stream in listener.incoming()
	{
		let mut stream = stream.unwrap();
		let resp = Response::new(500, String::from("Something happened!"));
		let write_res = stream.write(&(resp.to_vec()));
    }

	```
	*/
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
					warn!("Using default error page because we couldn't find error.html - {}",e);
					String::from("<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'><title>{}</title></head><body><h1>{}</h1><p>{}</p></body></html>")
				},
				Ok(body) => body
			};
			error_page = error_page.replacen("{}", &status, 2);
			let error_descr = String::from_utf8_lossy(&self.body);
			error_page.replacen("{}", &error_descr, 1).as_bytes().to_vec()
		}else{
			self.body.to_owned()
		};

		let mut out = (format!("HTTP/1.1 {}\r\nContent-Type: {};\r\nContent-Length: {};\r\n\r\n", status, self.mime, body_out.len())).as_bytes().to_vec();
		out.append(&mut body_out);
		out
	}

	/**
	Send this response out over the given stream.

	# Parameters
	- `stream`: The stream to which we write the response

	# Examples
	```no_run
	use c20web::Response;
	use std::net::TcpListener;
	use std::io::Write;

	let listener = TcpListener::bind("127.0.0.1:8000").unwrap();
    for mut stream in listener.incoming()
	{
		let mut stream = stream.unwrap();
		let resp = Response::new(500, String::from("Something happened!"));
		resp.send(stream);
    }
	```
	*/
	pub fn send(&self, mut stream: TcpStream)
	{
		let write_res = stream.write(&(self.to_vec()));
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
}

/**
Represents the "resource" portion of the first line of an HTTP Request.
*/
pub struct ResourcePath
{
	pub resource: String
}

impl ResourcePath
{
	/**
	Get the local filesystem path of the resource. Does not check for
	its existence, just returns the path that it *should* be located at.

	# Parameters
	- `webroot`: Filesystem path to the web root.

	# Returns
	Local filesystem path of the resource

	# Examples
	```
	use c20web::ResourcePath;

	let res = ResourcePath{resource: String::from("/hello.jpg")};
	let webroot = String::from("/var/www/myWebsite");
	let path = res.get_path(webroot);

	assert_eq!(path, String::from("/var/www/myWebsite/hello.jpg"));
	```
	*/
	pub fn get_path(&self, webroot: String) -> String
	{
		let path = self.resource.replacen(&"/",&"",1);
		format!("{}/{}", webroot, path)
	}

	/**
	Get the extension of the file indicated by this resource string. This is
	mainly for later determination of the MIME type, so if there is any
	problem determining the extension, we just default to the empty string.

	# Returns
	The file extension.

	# Examples
	```
	use c20web::ResourcePath;

	let res = ResourcePath{resource: String::from("/hello.jpg")};
	let extension = res.get_extension();
	assert_eq!(extension, String::from("jpg"));
	```
	*/
	pub fn get_extension(&self) -> String
	{
		match Path::new(&self.resource).extension(){
			Some(x) => match x.to_str(){
					Some(xs) => String::from(xs),
					None => String::from("")
				},
			None => String::from("")
		}
	}

	/**
	# Returns
	The MIME type associated with the extension of the file indicated by
	this resource.

	# Examples
	```
	use c20web::ResourcePath;

	let res = ResourcePath{resource: String::from("/hello.jpg")};
	let mime = res.get_mime();
	assert_eq!(mime, String::from("image/jpeg"));
	```
	*/
	pub fn get_mime(&self) -> &str
	{
		let extension = self.get_extension();
		if let Some(found_mime) = MIME_BY_EXTENSION.get(&extension)
		{
			found_mime
		}else{
			warn!("Could not find MIME type for file extension: {}", extension);
			"text/plain"
		}
	}
}

/**
Handle an incoming TCP connection. This is the function that gets loaded into
a thread with each new connection. Handles everything including output,
logging, and cleanup.

# Parameters
- `stream`: The TCP Stream of the connection we are to handle

# Examples
```no_run
use std::net::TcpListener;
use threadpool::ThreadPool;
use c20web::handle_connection;

let listener = TcpListener::bind("127.0.0.1:8000").unwrap();
let pool = ThreadPool::new(100);
for stream in listener.incoming()
{
	let stream = stream.unwrap();
	pool.execute(move ||{handle_connection(stream);});
}
```
*/
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
							let res = ResourcePath{resource: request.resource};
							let path = res.get_path(webroot);
							trace!("Requesting page: {}",&path);
							let mime = res.get_mime();
							match std::fs::read(&path)
							{
								Ok(bytes) => Response{code: 200, mime: String::from(mime), body: bytes},
								Err(e) => Response::new(404, format!("{}",e))
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

	//send output
	response.send(stream);
}

/*
Test those functions which weren't able to have good tests as part of their
example usage in the docs, but are still possible to unit-test
*/
#[cfg(test)]
mod tests
{
	use super::*;

	// Request::parse
	#[test]
	fn parse_request()
	{
		let req_string = Box::new(b"GET /hello.htm HTTP/1.1\r\nUser-Agent: Mozilla/4.0 (compatible; MSIE5.01; Windows NT)\r\nHost: 127.0.0.1:8000\r\n\r\n".to_owned());
		let request = Request::parse(req_string);
		match request
		{
			Ok(req) =>{
				assert_eq!(req.method, "GET");
				assert_eq!(req.resource, "/hello.htm");
				assert_eq!(req.http_version, "HTTP/1.1");
			}
			Err(_) => assert!(false)
		}
	}

	// Response.to_vec
	#[test]
	fn response_to_vec()
	{
		let body = String::from("<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'><title>Hello</title></head><body><h1>Hello</h1><p>Greetings from Rust</p></body></html>").as_bytes().to_vec();
		let res = Response{code: 200, mime: String::from("text/html"), body};
		let out_vec = res.to_vec();

		let out_expected = b"HTTP/1.1 200 OK\r\nContent-Type: text/html;\r\nContent-Length: 146;\r\n\r\n<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'><title>Hello</title></head><body><h1>Hello</h1><p>Greetings from Rust</p></body></html>".to_vec();
		assert_eq!(out_vec, out_expected);
	}
}