use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

// 定义一些常量
const LISTEN_ADDRESS: &str = "127.0.0.1:8080";
const MESSAGE_SIZE: usize = 1024;

// 发送的消息类型
type ReceiverMessage = Receiver<Message>;
type SenderMessage = Sender<Message>;

// 主要记录当前用户ID
struct User {
    user_id: u128,
    username: String,
    socket: TcpStream,
}

impl User {
    fn new(username: String, socket: TcpStream) -> User {
        User {
            user_id: User::gen_user_id(),
            socket,
            username,
        }
    }

    // 生成一个用户ID
    fn gen_user_id() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}

// 记录发送消息的用户，和消息内容
struct Message {
    id: u128,
    message: String,
}

fn sleep() {
    // 防止程序空转
    // 时间不宜过长，收发消息会慢
    thread::sleep(std::time::Duration::from_millis(10));
}

// 启动一个线程，读取启动的socket发送的消息，通过sender发送出去
fn read_to_transfer(
    mut socket: TcpStream,
    sender: SenderMessage,
    address: SocketAddr,
    user_id: u128,
) {
    thread::spawn(move || loop {
        // 消息缓冲大小
        let mut buf = vec![0; MESSAGE_SIZE];
        match socket.read_exact(&mut buf) {
            Ok(_) => {
                // 将无意义的0去除，并将消息转为string
                let msg = match String::from_utf8(
                    buf.into_iter().take_while(|&x| x != 0).collect::<Vec<_>>(),
                ) {
                    Ok(msg) => msg,
                    Err(_) => {
                        println!("不支持的utf8消息，无法转发");
                        continue;
                    }
                };
                println!("{}: {}", address, msg);

                // 所有客户端将消息发送给接收者，由接收者统一处理转发
                match sender.send(Message {
                    id: user_id,
                    message: msg,
                }) {
                    Ok(_) => {}
                    Err(_) => {
                        println!("内部消息转发失败！");
                    }
                };
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(_) => {
                // 一切超出预期的异常，都被视为客户端终止链接。
                println!("客户端 {} 离线！", address);
                break; // 此时线程结束
            }
        }
        sleep();
    });
}

// 将收到的消息转发给各个客户端，不包含发送者本身。
fn transfer_message(
    mut users: Vec<User>,
    receiver: ReceiverMessage,
) -> (Vec<User>, ReceiverMessage) {
    if let Ok(Message { id, message }) = receiver.try_recv() {
        users = users
            .into_iter()
            .filter(|user| {
                // 排除发送者本身
                if user.user_id == id {
                    return true;
                }
                let mut buf = format!("{}: {}", user.username, message.clone()).into_bytes();
                buf.resize(MESSAGE_SIZE, 0);

                // 这一步可以过滤掉发送消息失败的socket，
                // 这说明当前socket已经不再连接服务器。
                let mut socket = &user.socket;
                socket.write_all(&buf).map(|_| socket).is_ok()
            })
            .collect::<Vec<User>>();
    }
    // 将所有权再转移出去
    (users, receiver)
}

fn get_username(mut socket: &mut TcpStream, address: &SocketAddr) -> String {
    socket = socket
        .write_all("用户名：".as_bytes())
        .map(|_| socket)
        .unwrap();

    loop {
        // 消息缓冲大小
        let mut buf = vec![0; MESSAGE_SIZE];
        match socket.read_exact(&mut buf) {
            Ok(_) => {
                // 将无意义的0去除，并将消息转为string
                match String::from_utf8(buf.into_iter().take_while(|&x| x != 0).collect::<Vec<_>>())
                {
                    Ok(username) => {
                        println!("Welcome {}@{}", username, address);
                        break username;
                    }
                    Err(_) => {
                        println!("不支持的utf8消息，无法转发");
                        continue;
                    }
                };
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(_) => {
                // 一切超出预期的异常，都被视为客户端终止链接。
                println!("客户端 {} 离线！", address);
                break "".to_string(); // 此时线程结束
            }
        }
        sleep();
    }
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(LISTEN_ADDRESS)?;
    listener.set_nonblocking(true)?;
    println!("Listening Address: {}", LISTEN_ADDRESS);

    // 存储所有的连接用户
    let mut users: Vec<User> = vec![];
    // 用来发送消息和接收消息的句柄
    let (sender, mut receiver) = mpsc::channel::<Message>();

    loop {
        if let Ok((mut socket, address)) = listener.accept() {
            let socket_clone = match socket.try_clone() {
                Ok(socket) => socket,
                Err(_) => {
                    println!("克隆 {} 链接失败！", address);
                    continue;
                }
            };

            let username = get_username(&mut socket, &address);
            if username.is_empty() {
                println!("未获取到链接{}的用户名！", address);
                continue;
            }

            // 生成一个用户，包含一些关键信息
            let user = User::new(username, socket_clone);
            let user_id = user.user_id;
            users.push(user);

            read_to_transfer(socket, sender.clone(), address, user_id);
        }
        // stable 暂不支持 (x, y) = function(); 的形式
        // 处理消息，将消息转发
        let result = transfer_message(users, receiver);
        // 回收所有权
        users = result.0;
        receiver = result.1;

        sleep();
    }
}
