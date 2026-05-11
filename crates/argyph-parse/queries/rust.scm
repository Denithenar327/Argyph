;; Rust symbol extraction queries
;; Captures @name for symbol identifiers, @def for definition nodes

;; Function definitions
(function_item
  name: (identifier) @name
  parameters: (parameters)
  return_type: (_)?) @def

;; Struct definitions
(struct_item
  name: (type_identifier) @name) @def

;; Enum definitions
(enum_item
  name: (type_identifier) @name) @def

;; Trait definitions
(trait_item
  name: (type_identifier) @name) @def

;; Impl blocks (has no name; capture type name instead)
(impl_item
  type: (_) @name) @def

;; Module declarations
(mod_item
  name: (identifier) @name) @def

;; Macro definitions
(macro_definition
  name: (identifier) @name) @def

;; Constant items
(const_item
  name: (identifier) @name) @def

;; Static items
(static_item
  name: (identifier) @name) @def

;; Type aliases
(type_item
  name: (type_identifier) @name) @def

;; Use declarations (imports — extracted separately)
(use_declaration) @import

;; Extern crate declarations
(extern_crate_declaration) @import
