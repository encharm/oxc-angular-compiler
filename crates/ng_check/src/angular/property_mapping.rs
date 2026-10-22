//! `packages/compiler/src/property_mapping.ts`

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

// property_mapping.ts:14 ClassPropertyName
pub type ClassPropertyName = String;

// property_mapping.ts:21 BindingPropertyName
pub type BindingPropertyName = String;

// property_mapping.ts:27 InputOrOutput
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputOrOutput {
    pub class_property_name: ClassPropertyName,
    pub binding_property_name: BindingPropertyName,
    pub is_signal: bool,
}

/// What a mapping's values are: `InputOrOutput`, or a richer type (`TcbInputMapping`) that has one.
pub trait AsInputOrOutput {
    fn input_or_output(&self) -> &InputOrOutput;
}

impl AsInputOrOutput for InputOrOutput {
    fn input_or_output(&self) -> &InputOrOutput {
        self
    }
}

type Map<K, V> = IndexMap<K, V, FxBuildHasher>;

// property_mapping.ts:54 ClassPropertyMapping
#[derive(Debug, Clone)]
pub struct ClassPropertyMapping<T = InputOrOutput> {
    forward_map: Map<ClassPropertyName, T>,
    reverse_map: Map<BindingPropertyName, Vec<T>>,
}

impl<T: AsInputOrOutput + Clone> ClassPropertyMapping<T> {
    // property_mapping.ts:65 constructor
    fn new(forward_map: Map<ClassPropertyName, T>) -> Self {
        let reverse_map = reverse_map_from_forward_map(&forward_map);
        Self { forward_map, reverse_map }
    }

    // property_mapping.ts:73 empty
    pub fn empty() -> Self {
        Self::new(Map::default())
    }

    /// property_mapping.ts:82 fromMappedObject, for the values that are already `T` (the string shorthand is
    /// `from_binding_names`)
    pub fn from_entries(entries: impl IntoIterator<Item = (ClassPropertyName, T)>) -> Self {
        Self::new(entries.into_iter().collect())
    }

    // property_mapping.ts:108 merge
    pub fn merge(a: &Self, b: &Self) -> Self {
        let mut forward_map = a.forward_map.clone();
        for (class_property_name, input_or_output) in &b.forward_map {
            forward_map.insert(class_property_name.clone(), input_or_output.clone());
        }
        Self::new(forward_map)
    }

    // property_mapping.ts:128 classPropertyNames
    pub fn class_property_names(&self) -> Vec<ClassPropertyName> {
        self.forward_map.keys().cloned().collect()
    }

    // property_mapping.ts:135 propertyNames
    pub fn property_names(&self) -> Vec<BindingPropertyName> {
        self.reverse_map.keys().cloned().collect()
    }

    // property_mapping.ts:142 hasBindingPropertyName
    pub fn has_binding_property_name(&self, property_name: &str) -> bool {
        self.reverse_map.contains_key(property_name)
    }

    // property_mapping.ts:149 getByBindingPropertyName
    pub fn get_by_binding_property_name(&self, property_name: &str) -> Option<&[T]> {
        self.reverse_map.get(property_name).map(Vec::as_slice)
    }

    // property_mapping.ts:156 getByClassPropertyName
    pub fn get_by_class_property_name(&self, class_property_name: &str) -> Option<&T> {
        self.forward_map.get(class_property_name)
    }

    // property_mapping.ts:164 toDirectMappedObject
    pub fn to_direct_mapped_object(&self) -> Map<ClassPropertyName, BindingPropertyName> {
        self.forward_map
            .iter()
            .map(|(k, v)| (k.clone(), v.input_or_output().binding_property_name.clone()))
            .collect()
    }

    // property_mapping.ts:193 [Symbol.iterator]
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.forward_map.values()
    }
}

impl ClassPropertyMapping<InputOrOutput> {
    /// property_mapping.ts:82 fromMappedObject with the string shorthand: class property name to binding name, not
    /// signal based.
    pub fn from_binding_names(entries: impl IntoIterator<Item = (ClassPropertyName, BindingPropertyName)>) -> Self {
        Self::from_entries(entries.into_iter().map(|(class_property_name, binding_property_name)| {
            let value = InputOrOutput {
                class_property_name: class_property_name.clone(),
                binding_property_name,
                is_signal: false,
            };
            (class_property_name, value)
        }))
    }
}

// property_mapping.ts:200 reverseMapFromForwardMap
fn reverse_map_from_forward_map<T: AsInputOrOutput + Clone>(
    forward_map: &Map<ClassPropertyName, T>,
) -> Map<BindingPropertyName, Vec<T>> {
    let mut reverse_map: Map<BindingPropertyName, Vec<T>> = Map::default();
    for input_or_output in forward_map.values() {
        reverse_map
            .entry(input_or_output.input_or_output().binding_property_name.clone())
            .or_default()
            .push(input_or_output.clone());
    }
    reverse_map
}
