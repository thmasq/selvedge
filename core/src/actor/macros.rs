#[macro_export]
macro_rules! define_handler_group {
    ($group_name:ident) => {
        paste::paste! {
            selvedge_shared::[<$group_name:snake _message_modules>]!(
                $crate::__define_handler_group_impl
            );
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __define_handler_group_impl {
    ($group_name:ident => [ $($mod_name:ident),* $(,)? ]) => {
        $(
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
                                let fut = $mod_name::run(self.actor, args);
                                let result: Vec<selvedge_shared::event::ToShell> = fut.await;
                                result
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
    () => {
        selvedge_shared::top_level_message_modules!($crate::__define_top_level_dispatcher_impl);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __define_top_level_dispatcher_impl {
    ([ $($mod_name:ident),* $(,)? ]) => {
        $(
            pub mod $mod_name;
        )*

        paste::paste! {
            impl $crate::actor::MatrixActor {
                pub(crate) async fn handle_message(
			&self,
			msg: selvedge_shared::message::ToActor
                ) -> Vec<selvedge_shared::event::ToShell> {
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
