use super::ImeManager;
use zbus::{blocking::Connection, zvariant::OwnedObjectPath};

enum ImeService {
    IBus(zbus::blocking::Proxy<'static>),
    Fcitx5(zbus::blocking::Proxy<'static>),
    None,
}

pub struct LinuxImeManager {
    service: ImeService,
}

impl LinuxImeManager {
    pub fn new() -> Self {
        let service = if let Ok(connection) = Connection::session() {
            if let Ok(proxy) = zbus::blocking::Proxy::new(
                &connection,
                "org.freedesktop.IBus",
                "/org/freedesktop/IBus/InputContexts",
                "org.freedesktop.IBus.InputContext",
            ) {
                ImeService::IBus(proxy)
            } else if let Ok(proxy) = zbus::blocking::Proxy::new(
                &connection,
                "org.fcitx.Fcitx5",
                "/org/fcitx/Fcitx5/InputContext1",
                "org.fcitx.Fcitx5.InputContext1",
            ) {
                ImeService::Fcitx5(proxy)
            } else {
                ImeService::None
            }
        } else {
            ImeService::None
        };

        Self { service }
    }
}

impl ImeManager for LinuxImeManager {
    fn disable_and_get_status(&mut self) -> bool {
        let mut was_ime_enabled = false;
        match &self.service {
            ImeService::IBus(proxy) => {
                if let Ok(active) = proxy
                    .call_method("IsActive", &())
                    .and_then(|r| r.body().deserialize())
                {
                    was_ime_enabled = active;
                }
                let _ = proxy.call_method("FocusOut", &());
            }
            ImeService::Fcitx5(proxy) => {
                if let Ok(active) = proxy
                    .call_method("IsActive", &())
                    .and_then(|r| r.body().deserialize())
                {
                    was_ime_enabled = active;
                }
                let _ = proxy.call_method(
                    "SetInputMethod",
                    &("", OwnedObjectPath::from(zbus::zvariant::ObjectPath::try_from("/").unwrap())),
                );
            }
            ImeService::None => (),
        }
        was_ime_enabled
    }

    fn enable_with_status(&mut self, status: Option<bool>) {
        if let Some(true) = status {
            match &self.service {
                ImeService::IBus(proxy) => {
                    let _ = proxy.call_method("FocusIn", &());
                }
                ImeService::Fcitx5(proxy) => {
                    let _ = proxy.call_method("FocusIn", &());
                }
                ImeService::None => (),
            }
        }
    }
}