use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    process::exit,
    sync::mpsc::{self, Receiver},
    thread, vec,
};

const SERVER: &str = "127.0.0.1:8080";
const MESSAGE_SIZE: usize = 1024;

fn sleep() {
    thread::sleep(std::time::Duration::from_millis(10));
}

fn transfer_message(mut socket: TcpStream, receiver: Receiver<String>) {
    thread::spawn(move || loop {
        let mut buf = vec![0; MESSAGE_SIZE];
        match socket.read_exact(&mut buf) {
            Ok(_) => {
                let message = buf.into_iter().take_while(|&x| x != 0).collect::<Vec<_>>();
                let message = String::from_utf8(message).expect("非预期的utf8字符串");
                println!("\r<<< {}", message);
                print!(">>> ");
                std::io::stdout().flush().expect("刷新消息失败！");
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(_err) => {
                // 服务器关闭，关闭客户端。
                println!("服务器关闭！");
                exit(1);
            }
        };

        match receiver.try_recv() {
            Ok(msg) => {
                let mut buf = msg.into_bytes();
                buf.resize(MESSAGE_SIZE, 0);
                socket.write_all(&buf).expect("客户端发送消息异常");
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        };
        sleep();
    });
}

fn main() {
    // 连接上服务器
    let socket = TcpStream::connect(SERVER)
        .expect(format!("连接服务 {} 失败，请检查服务是否开启", SERVER).as_str());
    socket.set_nonblocking(true).expect("设置非阻塞失败");

    //
    let (sender, receiver) = mpsc::channel::<String>();

    // 启动一个线程，用来处理服务器发送过来的消息
    // 并将当前客户端发送的消息发送给服务器
    transfer_message(socket, receiver);

    loop {
        let mut buf = String::new();
        print!(">>> ");
        let _ = std::io::stdout().flush();

        std::io::stdin()
            .read_line(&mut buf)
            .expect("无法正确读取当前客户端写入的消息");
        let message = buf.trim().to_string();

        // 如果当前消息为空，则不做任何处理
        // 如果当前消息为quit，或消息发送失败，则退出程序
        if !message.is_empty() && (message == "quit" || sender.send(message).is_err()) {
            break;
        }
    }
}
