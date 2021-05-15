use std::ffi::c_void;

use bindings::Windows::Win32::System::{
    Com::{
        CoSetProxyBlanket, EOAC_DEFAULT, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
    },
    OleAutomation::{VariantClear, BSTR, VARENUM, VARIANT, VT_BSTR},
    Rpc::{RPC_C_AUTHN_DEFAULT, RPC_C_AUTHZ_DEFAULT},
    SystemServices::PWSTR,
    Wmi::{
        IEnumWbemClassObject, IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator,
        WBEM_FLAG_CONNECT_USE_MAX_WAIT, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
        WBEM_INFINITE,
    },
};

#[derive(Debug)]
pub struct Wmi {
    namespace: IWbemServices,
}

assert_not_impl_any!(Wmi: Send, Sync);

impl Wmi {
    pub fn exec_query(&self, query: impl AsRef<str>) -> windows::Result<WmiObjectIterator> {
        unsafe {
            let mut enumerator = Default::default();
            self.namespace
                .ExecQuery(
                    BSTR::from("WQL"),
                    BSTR::from(query.as_ref()),
                    WBEM_FLAG_RETURN_IMMEDIATELY.0 | WBEM_FLAG_FORWARD_ONLY.0,
                    None,
                    &mut enumerator,
                )
                .ok()?;
            Ok(WmiObjectIterator {
                enumerator: enumerator.unwrap(),
            })
        }
    }
}

#[derive(Debug)]
pub struct WmiObjectIterator {
    enumerator: IEnumWbemClassObject,
}

assert_not_impl_any!(WmiObjectIterator: Send, Sync);

impl Iterator for WmiObjectIterator {
    type Item = windows::Result<WmiObject>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let mut object = Default::default();
            let mut returned = 0;
            let result = self
                .enumerator
                .Next(WBEM_INFINITE.0, 1, &mut object, &mut returned);
            if result.is_ok() {
                object.map(|object| Ok(WmiObject { object }))
            } else {
                Some(Err(result.ok().unwrap_err()))
            }
        }
    }
}

#[derive(Debug)]
pub struct WmiObject {
    object: IWbemClassObject,
}

assert_not_impl_any!(WmiObjectIterator: Send, Sync);

impl WmiObject {
    pub fn get(&self, name: impl AsRef<str>) -> windows::Result<Variant> {
        unsafe {
            let mut variant = std::mem::zeroed();
            self.object
                .Get(
                    name.as_ref(),
                    0,
                    &mut variant,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
                .ok()?;
            Ok(Variant { variant })
        }
    }
}

pub struct Variant {
    variant: VARIANT,
}

impl Variant {
    pub fn get_string(&self) -> Option<String> {
        unsafe {
            if VARENUM(self.variant.Anonymous.Anonymous.vt.into()) != VT_BSTR {
                return None;
            }

            let ptr = self.variant.Anonymous.Anonymous.Anonymous.bstrVal;

            let string = widestring::U16CString::from_ptr_str(ptr);

            Some(string.to_string().unwrap())
        }
    }
}

impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            VariantClear(&mut self.variant).unwrap();
        }
    }
}

#[derive(Debug, Clone)]
pub struct WmiConnector {
    namespace: String,
    user: Option<String>,
    password: Option<String>,
    locale: Option<String>,
    use_max_wait: bool,
    authority: Option<String>,
}

impl WmiConnector {
    pub fn new(namespace: impl AsRef<str>) -> Self {
        Self {
            namespace: namespace.as_ref().to_string(),
            user: None,
            password: None,
            locale: None,
            use_max_wait: false,
            authority: None,
        }
    }

    pub fn user(mut self, user: impl AsRef<str>) -> Self {
        self.user = Some(user.as_ref().to_string());
        self
    }

    pub fn password(mut self, password: impl AsRef<str>) -> Self {
        self.password = Some(password.as_ref().to_string());
        self
    }

    pub fn locale(mut self, locale: impl AsRef<str>) -> Self {
        self.locale = Some(locale.as_ref().to_string());
        self
    }

    pub fn authority(mut self, authority: impl AsRef<str>) -> Self {
        self.authority = Some(authority.as_ref().to_string());
        self
    }

    pub fn connect(&self) -> windows::Result<Wmi> {
        let locator: IWbemLocator = windows::create_instance(&WbemLocator)?;

        unsafe {
            let mut services = Default::default();
            locator
                .ConnectServer(
                    BSTR::from(&self.namespace),
                    self.user.as_ref().map(BSTR::from),
                    self.password.as_ref().map(BSTR::from),
                    self.locale.as_ref().map(BSTR::from),
                    if self.use_max_wait {
                        WBEM_FLAG_CONNECT_USE_MAX_WAIT.0
                    } else {
                        0
                    },
                    self.authority.as_ref().map(BSTR::from),
                    None,
                    &mut services,
                )
                .ok()?;

            let services = services.unwrap();

            const COLE_DEFAULT_PRINCIPAL: *mut u16 = -1 as _;
            const COLE_DEFAULT_AUTHINFO: *mut c_void = -1 as _;

            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_DEFAULT as _,
                RPC_C_AUTHZ_DEFAULT,
                PWSTR(COLE_DEFAULT_PRINCIPAL),
                RPC_C_AUTHN_LEVEL_DEFAULT,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                COLE_DEFAULT_AUTHINFO,
                EOAC_DEFAULT,
            )
            .ok()?;

            Ok(Wmi {
                namespace: services,
            })
        }
    }
}
