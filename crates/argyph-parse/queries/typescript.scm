;; TypeScript symbol extraction queries
;; Captures @name for symbol identifiers, @def for definition nodes

;; Function declarations
(function_declaration
  name: (identifier) @name) @def

;; Generator function declarations
(generator_function_declaration
  name: (identifier) @name) @def

;; Method definitions
(method_definition
  name: (property_identifier) @name) @def

;; Class declarations
(class_declaration
  name: (type_identifier) @name) @def

;; Interface declarations
(interface_declaration
  name: (type_identifier) @name) @def

;; Type alias declarations
(type_alias_declaration
  name: (type_identifier) @name) @def

;; Enum declarations
(enum_declaration
  name: (identifier) @name) @def

;; Arrow functions assigned to variables (top-level)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression)])) @def

;; Variable declarations at top-level that aren't arrow functions
;; (captured for const/let top-level assignables)
(export_statement
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: [(arrow_function) (function_expression)]))) @def

;; Import statements (extracted separately)
(import_statement) @import

;; Export statements with specifiers
(export_statement
  (export_clause)) @import
