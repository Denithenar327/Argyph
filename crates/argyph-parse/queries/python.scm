;; Python symbol extraction queries
;; Captures @name for symbol identifiers, @def for definition nodes

;; Function definitions
(function_definition
  name: (identifier) @name) @def

;; Class definitions
(class_definition
  name: (identifier) @name) @def

;; Decorated definitions (unwrap the inner definition)
(decorated_definition
  definition: [
    (function_definition name: (identifier) @name)
    (class_definition name: (identifier) @name)
  ]) @def

;; Import statements (extracted separately)
(import_statement) @import

;; Import from statements
(import_from_statement) @import
