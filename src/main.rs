use std::env;
use telegram_bot::*;
use futures::StreamExt;
use log::{debug, info, warn};
use redis::Connection;
use redis::Commands;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv().ok();
    env_logger::init();
    let client = redis::Client::open(
        env::var("REDIS_URL").unwrap_or("redis://127.0.0.1/".to_string())
    ).unwrap();
    let mut redis_connection = client.get_connection().unwrap();

    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not found");
    let api = Api::new(token);

    let mut stream = api.stream();

    let add_status = vec![ChatMemberStatus::Member, ChatMemberStatus::Creator, ChatMemberStatus::Administrator];
    let del_status = vec![ChatMemberStatus::Kicked, ChatMemberStatus::Left];

    while let Some(update) = stream.next().await {
        if let Err(e) = update {
            eprintln!("Error: {}", e);
            continue;
        }

        let update = update.unwrap();
        debug!("Got update: {:?}", update);
        match update.kind {
            UpdateKind::Message(message) => {
                if let MessageKind::Text { ref data, .. } = message.kind {
                    if data == "/start" {
                        api.spawn(message.chat.text("Привет!").reply_markup(
                            ReplyKeyboardMarkup::from(vec![vec![
                                KeyboardButton::new("Моя очередь")
                            ]])
                        ));
                    }
                    if data == "Моя очередь" {
                        api.spawn(message.chat.text("Уже подошла?").reply_markup(
                            InlineKeyboardMarkup::from(vec![
                                vec![
                                    InlineKeyboardButton::callback("Да", "apply_buff")
                                ],
                                vec![
                                    InlineKeyboardButton::callback("Нет", "delete_request")
                                ],
                            ])
                        ));
                    }
                    remember_user(&mut redis_connection, &message.from.id, &message.chat.id());
                }

                if let MessageKind::NewChatMembers { ref data, .. } = message.kind {
                    let names = data.iter().map(|member| {
                        member.first_name.to_string()
                    }).collect::<Vec<String>>().join(", ");
                    let ids = data.iter().map(|member| {
                        member.id
                    }).collect::<Vec<UserId>>();
                    let chat_id = match message.chat.to_chat_ref() {
                        ChatRef::Id(id) => id,
                        _ => continue,
                    };
                    info!("Users [{}] was added to chat {}", ids.iter().map(|i| i.to_string()).collect::<Vec<String>>().join(","), chat_id);
                    remember_users(&mut redis_connection, ids, &chat_id);
                    api.spawn(message.chat.text(format!("Приветствую в новой группе, {}!", names))
                        //TODO: to remove
                        .reply_markup(
                            ReplyKeyboardMarkup::from(vec![vec![
                                KeyboardButton::new("Моя очередь")
                            ]])
                        ));
                }

                if let MessageKind::LeftChatMember { ref data, .. } = message.kind {
                    let id = data.id;
                    let chat_id = match message.chat.to_chat_ref() {
                        ChatRef::Id(id) => id,
                        _ => continue,
                    };
                    info!("User {} was deleted from chat {}", id, chat_id);
                    forget_user(&mut redis_connection, &id, &chat_id);
                    api.spawn(message.chat.text("Пока!"));
                }
            }

            UpdateKind::CallbackQuery(message) => {
                if let Some(ref data) = message.data {
                    if data == "apply_buff" {
                        let user = message.from.id;
                        if let Some(message) = &message.message {
                            let id = message.to_source_chat();
                            warn!("delete user {}. Req from chat {}", user, id);
                            save_user(&mut redis_connection, &api, &user);
                            api.spawn(id.text("Удачи!"));
                            api.spawn(message.delete());
                        }
                    }
                    if data == "delete_request" {
                        if let Some(message) = &message.message {
                            api.spawn(message.delete());
                        }
                    }
                }
            }

            UpdateKind::MyChatMember(chat_member) => {
                if del_status.contains(&chat_member.new_chat_member.status) {
                    let c_id: Integer = chat_member.chat.id().into();
                    info!("bot removed from {}", c_id);
                    //TODO: forget chat users
                }

                if add_status.contains(&chat_member.new_chat_member.status) {
                    let c_id = chat_member.chat.id();
                    info!("bot added to {}", c_id);
                    remember_chat_users(&mut redis_connection, &api, &c_id);
                }
            }

            _ => {}
        }
    }
    Ok(())
}

fn save_user(redis_connection: &mut Connection, api: &Api, user: &UserId) {
    info!("save user {}", user);
    let chats: Vec<Integer> = redis_connection.smembers(format!("chats_{}", user)).unwrap();
    info!("chats found {}", chats.iter().map(|i| i.to_string()).collect::<Vec<String>>().join(","));
    chats.iter().for_each(|chat| {
        api.spawn(KickChatMember::new(ChatId::new(chat.clone()), user));
    })
}

fn remember_user(redis_connection: &mut Connection, user: &UserId, chat: &ChatId) {
    let chat_id: Integer = chat.clone().into();
    redis_connection.sadd::<String, Integer, ()>(format!("chats_{}", user), chat_id).unwrap();
}

fn remember_users(redis_connection: &mut Connection, user: Vec<UserId>, chat: &ChatId) {
    let chat_id: Integer = chat.clone().into();
    user.iter().for_each(|u| {
        redis_connection.sadd::<String, Integer, ()>(format!("chats_{}", u), chat_id).unwrap();
    });
}

fn forget_user(redis_connection: &mut Connection, user: &UserId, chat: &ChatId) {
    let chat_id: Integer = chat.clone().into();
    redis_connection.srem::<String, Integer, ()>(format!("chats_{}", user), chat_id).unwrap();
}

#[allow(unused_variables)]
fn remember_chat_users(redis_connection: &mut Connection, api: &Api, chat: &ChatId) {
    info!("Getting users from chat {}", chat);
    //TODO: get users from chat
}