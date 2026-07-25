#[macro_export]
macro_rules! define_handler_group {
    (
        $group_name:ident {
            $(
                $(#[$meta:meta])*
                pub mod $mod_name:ident;
            )*
        }
    ) => {
        $(
            $(#[$meta])*
            pub mod $mod_name;
        )*

        paste::paste! {
            pub struct [<$group_name Handler>]<'a> {
                pub actor: &'a $crate::actor::MatrixActor,
            }

            impl<'a> [<$group_name Handler>]<'a> {
                pub async fn execute(
                    &self,
                    msg: selvedge_shared::message::[<$group_name:snake>]::[<$group_name Messages>]
                ) -> Vec<selvedge_shared::event::ToShell> {
                    match msg {
                        $(
                            selvedge_shared::message::[<$group_name:snake>]::[<$group_name Messages>]::[<$mod_name:camel>](args) => {
                                $mod_name::run(self.actor, args).await
                            }
                        )*
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_top_level_dispatcher {
    (
        $(
		$(#[$meta:meta])*
            pub mod $mod_name:ident;
        )*
    ) => {
        $(
            $(#[$meta])*
            pub mod $mod_name;
        )*

        paste::paste! {
            impl $crate::actor::MatrixActor {
                pub(crate) async fn handle_message(&self, msg: selvedge_shared::message::ToActor) -> Vec<selvedge_shared::event::ToShell> {
        			match msg {
                        $(
            				selvedge_shared::message::ToActor::[<$mod_name:camel>](action) => {
                                let handler = $crate::actor::handlers::$mod_name::[<$mod_name:camel Handler>] { actor: self };
                                handler.execute(action).await
            				}
                        )*
        			}
                }
            }
        }
    };
}
