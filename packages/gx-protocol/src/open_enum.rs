//! Open string enums: known variants plus `Other(String)`.

/// Declares a string enum that accepts any wire value.
///
/// Known values map to unit variants; anything else is kept verbatim in `Other`, so the value
/// survives a round trip and one unknown value cannot fail the frame that carries it.
macro_rules! open_string_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $wire:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)+
            /// A value this client does not know; kept verbatim.
            Other(String),
        }

        impl $name {
            /// Every known value, in declaration order (tables and tests walk it).
            pub const KNOWN: &'static [Self] = &[$(Self::$variant,)+];

            /// The wire spelling of this value.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value.as_str(),
                }
            }

            /// Maps a wire spelling to a variant; never fails.
            pub fn from_wire(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    other => Self::Other(other.to_string()),
                }
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct WireVisitor;
                impl<'de> ::serde::de::Visitor<'de> for WireVisitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        formatter.write_str(concat!("a string for ", stringify!($name)))
                    }

                    fn visit_str<E: ::serde::de::Error>(self, value: &str) -> Result<$name, E> {
                        Ok($name::from_wire(value))
                    }
                }
                deserializer.deserialize_str(WireVisitor)
            }
        }
    };
}
