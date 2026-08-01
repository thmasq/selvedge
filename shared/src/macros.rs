#[macro_export]
macro_rules! define_message_group {
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
            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            pub enum [<$group_name Messages>] {
                $(
                    [<$mod_name:camel>]( $mod_name::[<$mod_name:camel Args>] ),
                )*
            }

            #[macro_export]
            macro_rules! [<$group_name:snake _message_modules>] {
                ($callback:path) => {
                    $callback! { $group_name => [ $( $mod_name ),* ] }
                };
            }
        }
    };
}

#[macro_export]
macro_rules! define_top_level_messages {
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
            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            pub enum ToActor {
                $(
                    [<$mod_name:camel>]($mod_name::[<$mod_name:camel Messages>]),
                )*
            }

            #[macro_export]
            macro_rules! top_level_message_modules {
                ($callback:path) => {
                    $callback! { [ $( $mod_name ),* ] }
                };
            }
        }
    };
}

#[macro_export]
macro_rules! define_event_group {
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
            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            pub enum [<$group_name Events>] {
                $(
                    [<$mod_name:camel>]( $mod_name::[<$mod_name:camel Args>] ),
                )*
            }

            #[macro_export]
            macro_rules! [<$group_name:snake _event_modules>] {
                ($callback:path) => {
                    $callback! { $group_name => [ $( $mod_name ),* ] }
                };
            }
        }
    };
}

#[macro_export]
macro_rules! define_top_level_events {
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
            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            pub enum ToShell {
                $(
                    [<$mod_name:camel>]($mod_name::[<$mod_name:camel Events>]),
                )*
            }

            #[macro_export]
            macro_rules! top_level_event_modules {
                ($callback:path) => {
                    $callback! { [ $( $mod_name ),* ] }
                };
            }
        }
    };
}
