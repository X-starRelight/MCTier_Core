// MCTier_Core —— 基于 MCTier 项目开发
// 原作者：MCTier Team　原项目地址：https://github.com/pmh1314520/MCTier
// 原始代码 Copyright (c) 2024-2026 青云制作_彭明航（MCTier Team）
// 本项目修改与新增代码 Copyright (c) 2026 X-starRelight
// 许可证：见仓库根目录 LICENSE（MCTier 自定义源码可得非商业许可）
use super::shared::*;

// ==================== 大厅操作命令 ====================

/// 创建大厅
///
/// # 参数
/// * `name` - 大厅名称
/// * `password` - 大厅密码
/// * `player_name` - 玩家名称
/// * `player_id` - 玩家ID（由前端生成）
/// * `server_node` - 服务器节点地址
/// * `signaling_server` - 信令服务器地址
///
/// # 返回
/// * `Ok(Lobby)` - 成功创建的大厅信息
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn create_lobby(
    name: String,
    password: String,
    player_name: String,
    player_id: String,
    server_node: String,
    signaling_server: String,
    use_domain: Option<bool>,
    address_attempt: Option<u16>,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Lobby, String> {
    let password = crate::modules::secret_store::resolve(&password)?;
    require_secure_signaling(&signaling_server)?;
    log::info!(
        "收到创建大厅命令: name={}, player={}, player_id={}, signaling_server={}, use_domain={:?}",
        name,
        player_name,
        player_id,
        signaling_server,
        use_domain
    );

    let core = state.core.lock().await;

    // Claim the transition while holding core: overlapping requests must not
    // overwrite state or roll back a session owned by another request.
    if matches!(
        core.get_state().await,
        CoreAppState::Connecting | CoreAppState::InLobby
    ) {
        return Err("大厅正在连接或已连接，请先退出当前大厅".to_string());
    }
    // 更新应用状态为连接中
    core.set_state(CoreAppState::Connecting).await;

    // 【关键修复】在这里读取配置，避免在 start_easytier 中再次获取 core 的锁
    let (global_config, lobby_config) = {
        let config_manager = core.get_config_manager();
        let cfg_mgr = config_manager.lock().await;
        let user_config = cfg_mgr.get_config();

        let global_cfg = user_config.global_easytier_advanced_config.clone();
        let lobby_cfg = user_config.lobby_easytier_advanced_config.clone();

        (global_cfg, lobby_cfg)
    };

    // 获取各个服务的引用
    let lobby_manager = core.get_lobby_manager();
    let network_service = core.get_network_service();
    let p2p_signaling = core.get_p2p_signaling();
    let file_transfer = core.get_file_transfer();
    let chat_service = core.get_chat_service();

    // 释放 core 的锁，避免死锁
    drop(core);

    // 创建大厅
    let mut lobby_mgr = lobby_manager.lock().await;
    let network_svc = network_service.lock().await;

    match lobby_mgr
        .create_lobby_with_config(
            name,
            password,
            player_name.clone(),
            &player_id,
            server_node,
            signaling_server.clone(),
            use_domain.unwrap_or(false),
            &*network_svc,
            &app_handle,
            global_config,
            lobby_config,
            address_attempt.unwrap_or(0),
        )
        .await
    {
        Ok(lobby) => {
            log::info!("大厅创建成功: {}", lobby.name);

            // Never put the EasyTier/lobby secret into the persistent log.
            let mut log_lobby = lobby.clone();
            log_lobby.password = None;
            if let Ok(json) = serde_json::to_string(&log_lobby) {
                log::info!("大厅JSON: {}", json);
            }

            // 获取虚拟IP
            let virtual_ip = lobby.virtual_ip.clone();
            drop(lobby_mgr);
            drop(network_svc);

            log::info!("使用前端提供的玩家ID: {}", player_id);

            log::info!("客户端将连接到 WebSockets 信令服务器: {}", signaling_server);

            // 创建者也必须注册到远程信令服务器。此前只有加入大厅路径启动
            // P2P 信令，导致创建者没有向 Android/其他客户端发布大厅成员、
            // 聊天凭据和 WebRTC 信令，加入端会误判大厅不存在并自行成为房主。
            log::info!("正在启动P2P信令服务（创建大厅）...");
            let p2p_svc = p2p_signaling.lock().await;
            match p2p_svc
                .start(player_id, player_name, virtual_ip.clone())
                .await
            {
                Ok(_) => log::info!("✅ P2P信令服务启动成功（创建大厅）"),
                Err(e) => {
                    log::error!("❌ 启动P2P信令服务失败（创建大厅）: {}", e);
                    drop(p2p_svc);
                    lobby_manager.lock().await.force_clear_state();
                    let _ = network_service.lock().await.stop_easytier().await;
                    let core = state.core.lock().await;
                    core.set_state(CoreAppState::Error(format!("P2P信令服务启动失败: {}", e)))
                        .await;
                    drop(core);
                    return Err(format!("P2P信令服务启动失败: {}", e));
                }
            }
            drop(p2p_svc);

            // 不再在创建大厅时自动启动HTTP文件服务器
            // HTTP服务器将在第一次添加共享时按需启动
            log::info!("📝 HTTP文件服务器将在添加共享时按需启动");
            let ft_service = file_transfer.lock().await;
            ft_service.set_virtual_ip(virtual_ip.clone());
            drop(ft_service);

            // 聊天服务必须等待信令服务器下发 lobby token 后才能启动。
            let chat_svc = chat_service.lock().await;
            chat_svc.reset_for_lobby().await;
            chat_svc.set_virtual_ip(virtual_ip.clone());
            drop(chat_svc);

            // 更新应用状态为在大厅中
            let core = state.core.lock().await;
            core.set_state(CoreAppState::InLobby).await;
            drop(core);

            Ok(lobby)
        }
        Err(e) => {
            log::error!("创建大厅失败: {}", e);

            // Roll back partial setup.  EasyTier may already be running and
            // LobbyManager may already contain a lobby when the later P2P
            // signaling step fails; leaving either behind makes the next
            // click fail immediately with AlreadyInLobby.
            if matches!(e, crate::modules::lobby_manager::LobbyError::AlreadyInLobby) {
                drop(lobby_mgr);
                drop(network_svc);
                state
                    .core
                    .lock()
                    .await
                    .set_state(CoreAppState::InLobby)
                    .await;
                return Err(e.to_string());
            }
            lobby_mgr.force_clear_state();
            let _ = network_svc.stop_easytier().await;

            // 更新应用状态为错误
            drop(lobby_mgr);
            drop(network_svc);
            let core = state.core.lock().await;
            core.set_state(CoreAppState::Error(e.to_string())).await;
            drop(core);

            Err(e.to_string())
        }
    }
}

/// 加入大厅
///
/// # 参数
/// * `name` - 大厅名称
/// * `password` - 大厅密码
/// * `player_name` - 玩家名称
/// * `player_id` - 玩家ID（由前端生成）
/// * `server_node` - 服务器节点地址
/// * `signaling_server` - 信令服务器地址
///
/// # 返回
/// * `Ok(Lobby)` - 成功加入的大厅信息
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn join_lobby(
    name: String,
    password: String,
    player_name: String,
    player_id: String,
    server_node: String,
    signaling_server: String,
    use_domain: Option<bool>,
    address_attempt: Option<u16>,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Lobby, String> {
    let password = crate::modules::secret_store::resolve(&password)?;
    require_secure_signaling(&signaling_server)?;
    log::info!(
        "收到加入大厅命令: name={}, player={}, player_id={}, signaling_server={}, use_domain={:?}",
        name,
        player_name,
        player_id,
        signaling_server,
        use_domain
    );

    let core = state.core.lock().await;

    if matches!(
        core.get_state().await,
        CoreAppState::Connecting | CoreAppState::InLobby
    ) {
        return Err("大厅正在连接或已连接，请先退出当前大厅".to_string());
    }
    // 更新应用状态为连接中
    core.set_state(CoreAppState::Connecting).await;

    // 【关键修复】在这里读取配置，避免在 start_easytier 中再次获取 core 的锁
    let (global_config, lobby_config) = {
        let config_manager = core.get_config_manager();
        let cfg_mgr = config_manager.lock().await;
        let user_config = cfg_mgr.get_config();

        let global_cfg = user_config.global_easytier_advanced_config.clone();
        let lobby_cfg = user_config.lobby_easytier_advanced_config.clone();

        (global_cfg, lobby_cfg)
    };

    // 获取各个服务的引用
    let lobby_manager = core.get_lobby_manager();
    let network_service = core.get_network_service();
    let p2p_signaling = core.get_p2p_signaling();
    let file_transfer = core.get_file_transfer();
    let chat_service = core.get_chat_service();

    // 释放 core 的锁，避免死锁
    drop(core);

    // 加入大厅
    let mut lobby_mgr = lobby_manager.lock().await;
    let network_svc = network_service.lock().await;

    match lobby_mgr
        .join_lobby_with_config(
            name,
            password,
            player_name.clone(),
            &player_id,
            server_node,
            signaling_server.clone(),
            use_domain.unwrap_or(false),
            &*network_svc,
            &app_handle,
            global_config,
            lobby_config,
            address_attempt.unwrap_or(0),
        )
        .await
    {
        Ok(lobby) => {
            log::info!("成功加入大厅: {}", lobby.name);

            // 获取虚拟IP（用于P2P信令服务和HTTP文件服务器）
            let virtual_ip = lobby.virtual_ip.clone();
            drop(lobby_mgr);
            drop(network_svc);

            log::info!("使用前端提供的玩家ID: {}", player_id);

            log::info!("客户端将连接到 WebSockets 信令服务器: {}", signaling_server);

            // 启动P2P信令服务
            log::info!("正在启动P2P信令服务（加入大厅）...");
            let p2p_svc = p2p_signaling.lock().await;
            match p2p_svc
                .start(player_id, player_name, virtual_ip.clone())
                .await
            {
                Ok(_) => {
                    log::info!("✅ P2P信令服务启动成功（加入大厅）");
                }
                Err(e) => {
                    log::error!("❌ 启动P2P信令服务失败（加入大厅）: {}", e);
                    // P2P信令服务启动失败应该返回错误，因为没有它就无法发现其他玩家
                    drop(p2p_svc);
                    lobby_manager.lock().await.force_clear_state();
                    let _ = network_service.lock().await.stop_easytier().await;
                    let core = state.core.lock().await;
                    core.set_state(CoreAppState::Error(format!("P2P信令服务启动失败: {}", e)))
                        .await;
                    drop(core);
                    return Err(format!("P2P信令服务启动失败: {}", e));
                }
            }
            drop(p2p_svc);

            // 不再在加入大厅时自动启动HTTP文件服务器
            // HTTP服务器将在第一次添加共享时按需启动
            log::info!("📝 HTTP文件服务器将在添加共享时按需启动");
            let ft_service = file_transfer.lock().await;
            ft_service.set_virtual_ip(virtual_ip.clone());
            drop(ft_service);

            // 聊天服务必须等待信令服务器下发 lobby token 后才能启动。
            let chat_svc = chat_service.lock().await;
            chat_svc.reset_for_lobby().await;
            chat_svc.set_virtual_ip(virtual_ip.clone());
            drop(chat_svc);

            // 更新应用状态为在大厅中
            let core = state.core.lock().await;
            core.set_state(CoreAppState::InLobby).await;
            drop(core);

            Ok(lobby)
        }
        Err(e) => {
            log::error!("加入大厅失败: {}", e);

            // Roll back partial setup so a failed join can be retried without
            // requiring an application restart or a separate force-stop.
            if matches!(e, crate::modules::lobby_manager::LobbyError::AlreadyInLobby) {
                drop(lobby_mgr);
                drop(network_svc);
                state
                    .core
                    .lock()
                    .await
                    .set_state(CoreAppState::InLobby)
                    .await;
                return Err(e.to_string());
            }
            lobby_mgr.force_clear_state();
            let _ = network_svc.stop_easytier().await;

            // 更新应用状态为错误
            drop(lobby_mgr);
            drop(network_svc);
            let core = state.core.lock().await;
            core.set_state(CoreAppState::Error(e.to_string())).await;
            drop(core);

            Err(e.to_string())
        }
    }
}

/// 退出大厅
///
/// # 返回
/// * `Ok(())` - 成功退出
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn leave_lobby(state: State<'_, AppState>) -> Result<(), String> {
    log::info!("收到退出大厅命令");

    let core = state.core.lock().await;

    // 获取各个服务的引用
    let lobby_manager = core.get_lobby_manager();
    let network_service = core.get_network_service();
    let p2p_signaling = core.get_p2p_signaling();
    let file_transfer = core.get_file_transfer();
    let chat_service = core.get_chat_service();

    // 【修复】尽早释放 core 锁，避免在数秒级的 stop_easytier（netsh/pnputil/PowerShell）
    // 期间一直占用 core 锁，导致其它命令阻塞、界面卡死
    drop(core);

    // 先撤销聊天 token、身份映射和历史，再停止虚拟网络。
    let chat_svc = chat_service.lock().await;
    chat_svc.stop_server().await;
    drop(chat_svc);

    // 停止HTTP文件服务器
    let ft_service = file_transfer.lock().await;
    ft_service.stop_server().await;
    drop(ft_service);

    // 停止P2P信令服务
    let p2p_svc = p2p_signaling.lock().await;
    if let Err(e) = p2p_svc.stop().await {
        log::warn!("停止P2P信令服务失败: {}", e);
    }
    drop(p2p_svc);

    // 退出大厅
    let mut lobby_mgr = lobby_manager.lock().await;
    let network_svc = network_service.lock().await;

    match lobby_mgr.leave_lobby(&*network_svc).await {
        Ok(_) => {
            log::info!("成功退出大厅");
            drop(lobby_mgr);
            drop(network_svc);

            // 更新应用状态为空闲（重新短暂加锁）
            let core = state.core.lock().await;
            core.set_state(CoreAppState::Idle).await;
            drop(core);

            Ok(())
        }
        Err(e) => {
            log::error!("退出大厅失败: {}", e);
            Err(e.to_string())
        }
    }
}

// ==================== 配置管理命令 ====================

/// 获取用户配置
///
/// # 返回
/// * `Ok(UserConfig)` - 用户配置
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<UserConfig, String> {
    log::info!("收到获取配置命令");

    let core = state.core.lock().await;
    let config_manager = core.get_config_manager();
    let config_mgr = config_manager.lock().await;

    let mut config = config_mgr.get_config_clone();
    if let Some(auto) = config.auto_lobby.as_mut() {
        auto.lobby_password = auto
            .lobby_password
            .take()
            .map(crate::modules::secret_store::protect_lobby_password)
            .transpose()?;
    }

    Ok(config)
}

/// 更新用户配置
///
/// # 参数
/// * `config` - 新的用户配置
///
/// # 返回
/// * `Ok(())` - 更新成功
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn update_config(
    mut config: UserConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(auto) = config.auto_lobby.as_mut() {
        auto.lobby_password = auto
            .lobby_password
            .take()
            .map(crate::modules::secret_store::protect_lobby_password)
            .transpose()?;
    }
    log::info!("收到更新配置命令");

    let core = state.core.lock().await;
    let config_manager = core.get_config_manager();
    let mut config_mgr = config_manager.lock().await;

    match config_mgr
        .update_config(|cfg| {
            *cfg = config.clone();
        })
        .await
    {
        Ok(_) => {
            log::info!("配置已更新");
            Ok(())
        }
        Err(e) => {
            log::error!("更新配置失败: {}", e);
            Err(e.to_string())
        }
    }
}

// ==================== 系统信息命令 ====================

/// 获取当前应用状态
///
/// # 返回
/// * `Ok(String)` - 应用状态的字符串表示
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_app_state(state: State<'_, AppState>) -> Result<String, String> {
    let core = state.core.lock().await;
    let app_state = core.get_state().await;
    Ok(format!("{:?}", app_state))
}

/// 获取当前大厅信息
///
/// # 返回
/// * `Ok(Option<Lobby>)` - 当前大厅信息，如果未加入大厅则返回 None
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_current_lobby(state: State<'_, AppState>) -> Result<Option<Lobby>, String> {
    log::info!("收到获取当前大厅命令");

    let core = state.core.lock().await;
    let lobby_manager = core.get_lobby_manager();
    let lobby_mgr = lobby_manager.lock().await;

    let lobby = lobby_mgr.get_current_lobby().cloned();

    Ok(lobby)
}

/// 获取玩家列表
///
/// # 返回
/// * `Ok(Vec<Player>)` - 玩家列表
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_players(state: State<'_, AppState>) -> Result<Vec<Player>, String> {
    log::info!("收到获取玩家列表命令");

    let core = state.core.lock().await;
    let lobby_manager = core.get_lobby_manager();
    let lobby_mgr = lobby_manager.lock().await;

    let players = lobby_mgr.get_players();

    log::info!("返回 {} 个玩家", players.len());

    Ok(players)
}

/// 获取网络连接状态
///
/// # 返回
/// * `Ok(String)` - 连接状态的 JSON 字符串
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_network_status(state: State<'_, AppState>) -> Result<String, String> {
    let core = state.core.lock().await;
    let network_service = core.get_network_service();
    let network_svc = network_service.lock().await;

    let status = network_svc.check_connection().await;

    match serde_json::to_string(&status) {
        Ok(json) => Ok(json),
        Err(e) => Err(format!("序列化连接状态失败: {}", e)),
    }
}

/// 获取虚拟 IP 地址
///
/// # 返回
/// * `Ok(Option<String>)` - 虚拟 IP 地址，如果未连接则返回 None
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_virtual_ip(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let core = state.core.lock().await;
    let network_service = core.get_network_service();
    let network_svc = network_service.lock().await;

    let ip = network_svc.get_virtual_ip().await;

    Ok(ip)
}

/// 对等连接类型（虚拟IP -> p2p/relay）
#[derive(serde::Serialize)]
pub struct PeerConnType {
    pub ip: String,
    #[serde(rename = "connType")]
    pub conn_type: String,
    /// 链路延迟（毫秒，来自 EasyTier 自身统计），None 表示未知
    #[serde(rename = "latencyMs", skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// 累计接收字节（用于上层计算下行速率）
    #[serde(rename = "rxBytes", skip_serializing_if = "Option::is_none")]
    pub rx_bytes: Option<u64>,
    /// 累计发送字节（用于上层计算上行速率）
    #[serde(rename = "txBytes", skip_serializing_if = "Option::is_none")]
    pub tx_bytes: Option<u64>,
    /// 丢包率（百分比 0~100），None 表示未知
    #[serde(rename = "lossRate", skip_serializing_if = "Option::is_none")]
    pub loss_rate: Option<u8>,
}

/// 查询大厅内各对等节点的连接类型（P2P 直连 / 中继）。
/// 通过 easytier-cli 连接 easytier-core 的 RPC 端口获取 peer 路由，cost==1 即 P2P 直连。
#[tauri::command]
pub async fn get_peer_connection_types(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<PeerConnType>, String> {
    // 取当前 RPC 端口
    let rpc_port = {
        let core = state.core.lock().await;
        let ns = core.get_network_service();
        let svc = ns.lock().await;
        svc.get_rpc_port().await
    };
    let port = match rpc_port {
        Some(p) => p,
        None => return Ok(vec![]),
    };

    let cli_path =
        crate::modules::resource_manager::ResourceManager::get_easytier_cli_path(&app_handle)
            .map_err(|e| format!("获取 easytier-cli 失败: {}", e))?;

    let mut cmd = tokio::process::Command::new(&cli_path);
    cmd.args(["-p", &format!("127.0.0.1:{}", port), "-o", "json", "peer"]);
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = tokio::time::timeout(std::time::Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| "easytier-cli 查询超时".to_string())?
        .map_err(|e| format!("运行 easytier-cli 失败: {}", e))?;
    if !output.status.success() {
        return Ok(vec![]);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or(serde_json::Value::Null);

    // 递归收集所有含 ipv4 + cost 的对象（兼容单/多实例的 JSON 结构）
    let mut result: Vec<PeerConnType> = Vec::new();
    fn walk(v: &serde_json::Value, out: &mut Vec<PeerConnType>) {
        match v {
            serde_json::Value::Array(arr) => arr.iter().for_each(|x| walk(x, out)),
            serde_json::Value::Object(map) => {
                let ip = map.get("ipv4").and_then(|x| x.as_str()).unwrap_or("");
                let cost = map.get("cost").and_then(|x| x.as_str());
                if let (false, Some(cost)) = (ip.is_empty(), cost) {
                    if !cost.eq_ignore_ascii_case("local") {
                        let conn = if cost.eq_ignore_ascii_case("p2p") {
                            "p2p"
                        } else {
                            "relay"
                        };
                        // 从 stats 提取延迟/收发字节/丢包（字段名兼容大小写差异）
                        let stats = map.get("stats");
                        let latency_ms = stats
                            .and_then(|s| s.get("latency_us"))
                            .and_then(|v| v.as_u64())
                            .map(|us| us / 1000);
                        let rx_bytes = stats
                            .and_then(|s| s.get("rx_bytes"))
                            .and_then(|v| v.as_u64());
                        let tx_bytes = stats
                            .and_then(|s| s.get("tx_bytes"))
                            .and_then(|v| v.as_u64());
                        let loss_rate = map
                            .get("loss_rate")
                            .and_then(|v| v.as_f64())
                            .map(|f| ((f.clamp(0.0, 1.0)) * 100.0).round() as u8);
                        out.push(PeerConnType {
                            ip: ip.to_string(),
                            conn_type: conn.to_string(),
                            latency_ms,
                            rx_bytes,
                            tx_bytes,
                            loss_rate,
                        });
                    }
                }
                // 继续向下遍历（多实例结构里 peer 列表可能在子字段）
                map.values().for_each(|x| walk(x, out));
            }
            _ => {}
        }
    }
    walk(&parsed, &mut result);
    // 去重（同一 IP 保留首个）
    let mut seen = std::collections::HashSet::new();
    result.retain(|e| seen.insert(e.ip.clone()));
    Ok(result)
}

// ==================== 窗口控制命令 ====================

