//! This library provides containers where the keys are guaranteed to be valid.
//! The key is generated when a value is inserted into the container. That key
//! is guaranteed to only work with the map that generated it, and within that
//! container always reference the same value.
//!
//! ```
//! use provenance::ProvenanceMap;
//!
//! // A new map is easily created
//! let mut map = ProvenanceMap::<i32>::new().unwrap();
//!
//! // Inserting a value into the map returns a key
//! let key = map.insert(5);
//!
//! // That key is guaranteed to be able retrieve that value
//! assert_eq!(&5, map.get(key));
//! // Notice that the retrieved value is not wrapped in an Option or Result
//! ```
//!
//! Using a key with a map that did not create it is a compile time error:
//! ```compile_fail
//! use provenance::ProvenanceMap;
//!
//! let mut map1 = ProvenanceMap::<i32>::new().unwrap();
//! let mut map2 = ProvenanceMap::<bool>::new().unwrap();
//!
//! let key = map1.insert(5);
//! map2.get(key); // Using a key generated by map1 with map2 is a compilation error
//! ```
//!
//! # Map uniqueness
//! To be able to guarantee that only keys created by a map can be used with that map,
//! each map and key has a type parameter dedicated to denote _provenance_. However,
//! for every provenance, there may only exist a unique map, thus creating a map
//! may fail.
//! ```
//! use provenance::ProvenanceMap;
//!
//! // Creating a map is ok
//! let map = ProvenanceMap::<i32>::new();
//! assert!(map.is_some());
//!
//! // Creating another with the same provenance is not
//! let map = ProvenanceMap::<i32>::new();
//! assert!(map.is_none()); // Creation failed and `None` were returned
//! ```
//!
//! # Lightweight keys
//! The keys generated by this library's maps are ligthweight in the sense
//! that they are copiable. This means that other copiable values can link
//! to non-copiable values without worrying about lifetimes.
//! ```
//! use provenance::{Key, ProvenanceMap};
//! use std::ops::Add;
//!
//! struct Currency { name: String }
//!
//! #[derive(Copy, Clone)]
//! struct Money { amount: i32, currency: Key<Currency> }
//! impl Add for Money {
//!     type Output = Money;
//!
//!     fn add(self, rhs: Self) -> Self::Output {
//!         Money {
//!             amount: self.amount + rhs.amount,
//!             currency: self.currency,
//!         }
//!     }
//! }
//!
//! let mut currencies = ProvenanceMap::<Currency>::new().unwrap();
//! let sek = currencies.insert(Currency { name: "Swedish Krona".into() });
//!
//! let mon1 = Money { amount: 5, currency: sek };
//! let mon2 = Money { amount: 10, currency: sek };
//! let sum = mon1 + mon2;
//!
//! assert_eq!(sek, sum.currency);
//! assert_eq!("Swedish Krona".to_string(), currencies.get(sum.currency).name);
//! ```

use std::{
    collections::HashSet,
    marker::PhantomData,
    any::{TypeId},
    fmt::{Debug, Formatter},
    sync::Mutex,
    ops::DerefMut,
    hash::{Hash, Hasher}
};
use lazy_static::lazy_static;

/// A provenance map is a map-like data structure that know which keys belong
/// to which map.
///
/// Keys are generated upon inserting an element into the map.
/// ```
/// use provenance::ProvenanceMap;
/// let mut map = ProvenanceMap::<i32>::new().unwrap();
/// let key = map.insert(5);
/// ```
///
/// This is achieved by "tagging" keys with the generic type parameter of the map. So
/// a map of type `ProvenanceMap<i32>` will create keys of type `Key<i32>`. The key
/// does not actually contain a value of their generic type parameter. It is only used
/// to track what map the key came from, i.e. it's provenance.
/// ```compile_fail
/// use provenance::ProvenanceMap;
/// let mut map_1 = ProvenanceMap::<i32>::new().unwrap();
/// let key = map_1.insert(5);
/// let map_2 = ProvenanceMap::<bool>::new().unwrap();
///
/// // Using a key from another map is a type error.
/// map_2.get(key);
/// ```
///
/// However, if it were possible to create multiple maps with the same type, e.g.
/// `ProvenanceMap<String>` the type of the key wouldn't be enough to track what map
/// a key came from. Therefore, it is only possible to create a single map per
/// concrete type given for the `Value` type parameter.
/// ```
/// use provenance::ProvenanceMap;
///
/// // Creating a map once is OK
/// let mut map = ProvenanceMap::<String>::new();
/// assert!(map.is_some());
///
/// // Creating another map with the same signature is not OK
/// let mut map = ProvenanceMap::<String>::new();
/// assert!(map.is_none());
/// ```
///

pub struct ProvenanceMap<Value> {
    map: SeparateProvenanceMap<Value, Value>
}

impl<Value: 'static> ProvenanceMap<Value> {

    /// Create a new map if one with the given signature have not already been created.
    /// If one has, `None` is returned.
    /// ```
    /// use provenance::ProvenanceMap;
    ///
    /// // Creating a map once is OK
    /// let map = ProvenanceMap::<String>::new();
    /// assert!(map.is_some());
    ///
    /// // Creating another map with the same signature is not OK
    /// let map = ProvenanceMap::<String>::new();
    /// assert!(map.is_none());
    /// ```
    pub fn new() -> Option<ProvenanceMap<Value>> {
        let map = SeparateProvenanceMap::new()?;

        Some(ProvenanceMap {
            map
        })
    }

    /// Insert a value into the map.
    /// A key is generated for the value and returned.
    /// This key can be used to access the value later.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// let key = map.insert(5);
    /// assert_eq!(&5, map.get(key));
    /// ```
    ///
    /// Multiple equivalent values may be inserted into the map.
    /// Each will get a unique key.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// let key1 = map.insert(5);
    /// let key2 = map.insert(5);
    /// assert_ne!(key1, key2);
    /// ```
    pub fn insert(&mut self, value: Value) -> Key<Value> {
        self.map.insert(value)
    }

    /// Use a [key](Key) to retrieve an immutable reference to
    /// a stored value.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// let key = map.insert(15);
    /// assert_eq!(&15, map.get(key));
    /// ```
    pub fn get(&self, key: Key<Value>) -> &Value {
        self.map.get(key)
    }

    /// Use a [key](Key) to retrieve an mutable reference to
    /// a stored value.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// let key = map.insert(15);
    /// assert_eq!(&mut 15, map.get_mut(key));
    /// ```
    pub fn get_mut(&mut self, key: Key<Value>) -> &mut Value {
        self.map.get_mut(key)
    }

    /// Get an [iterator](Iterator) over all keys in the map.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(3, map.keys().count());
    /// ```
    pub fn keys(&self) -> impl Iterator<Item = Key<Value>> {
        self.map.keys()
    }

    /// Get an [iterator](Iterator) over immutable references to each value in the map.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(6, map.iter().sum());
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.map.iter()
    }

    /// Get an [iterator](Iterator) over mutable references to each value in the map.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// // Add one to every value
    /// map.iter_mut().for_each(|val| *val += 1);
    ///
    /// assert_eq!(9, map.iter().sum());
    /// ```
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.map.iter_mut()
    }

    /// Search the map in insertion order for the first value that satisfy the given predicate.
    /// If such value is found, an immutable reference to it is returned,
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(Some(&2), map.find(|&val| val == 2));
    /// ```
    /// otherwise `None` is returned.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(None, map.find(|&val| val == 53));
    /// ```
    pub fn find<P: Fn(&Value) -> bool>(&self, predicate: P) -> Option<&Value> {
        self.map.find(predicate)
    }

    /// Search the map in insertion order for the first value that satisfy the given predicate.
    /// If such value is found, a mutable reference to it is returned,
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(Some(&mut 2), map.find_mut(|&val| val == 2));
    /// ```
    /// otherwise [`None`](std::option::Option::None) is returned.
    /// ```
    /// use provenance::ProvenanceMap;
    /// let mut map = ProvenanceMap::<i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(None, map.find_mut(|&val| val == 53));
    /// ```
    pub fn find_mut<P: Fn(&Value) -> bool>(&mut self, predicate: P) -> Option<&mut Value> {
        self.map.find_mut(predicate)
    }
}

/// A [ProvenanceMap](ProvenanceMap) where the a type separate from the type of the stored
/// values can be used to signify the maps provenance.
///
/// This allows for multiple maps that store the values of the same type to be created,
/// as long as the type given for the `Provenance` parameter is unique.
/// ```
/// use provenance::{ProvenanceMap, SeparateProvenanceMap};
///
/// // Structs that only exist to denote provenance
/// struct One; struct Two;
///
/// // Creating a map once is OK
/// let mut map = SeparateProvenanceMap::<One, String>::new();
/// assert!(map.is_some());
///
/// // Creating another map is OK as long as the provenance is different
/// let mut map = SeparateProvenanceMap::<Two, String>::new();
/// assert!(map.is_some());
/// ```
///
/// A [ProvenanceMap](ProvenanceMap) can be thought of as a special case of this map
/// where the type of the stored values also is used as provenance. That is
/// `ProvenanceMap<i32> ≈ SeparateProvenanceMap<i32, i32>`. Currently, this is how
/// [ProvenanceMap](ProvenanceMap) is implemented. This has the effect that both
/// types share the available pool of types that can be used as provenance. That
/// means that both a `ProvenanceMap<i32>` and a `SeperateProvenanceMap<i32, B>`
/// for any type `B` can not be constructed. This beahviour may change in future
/// versions and should not be relied upon.
/// ```
/// use provenance::{ProvenanceMap, SeparateProvenanceMap};
///
/// // Creating a map once is OK
/// let mut map = ProvenanceMap::<i32>::new();
/// assert!(map.is_some());
///
/// // Creating another map with the same provenance as another
/// // is not OK, even if that map is a ProvenanceMap
/// let mut map = SeparateProvenanceMap::<i32, bool>::new();
/// assert!(map.is_none());
/// ```
pub struct SeparateProvenanceMap<Provenance, Value> {
    elements: Vec<Value>,
    _pd: PhantomData<Provenance>,
}

impl<Provenance: 'static, Value: 'static> SeparateProvenanceMap<Provenance, Value> {

    /// Creates a new empty map with some type as provenance.
    ///
    /// If a map with such provenance already has been created, `None` will be returned.
    /// Thus be careful to not drop maps unintentionally.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    ///
    /// struct Provenance;
    ///
    /// // Creating a map with some type as provenance is ok
    /// let map = SeparateProvenanceMap::<Provenance, bool>::new();
    /// assert!(map.is_some());
    ///
    /// // Creating another is not however
    /// let map = SeparateProvenanceMap::<Provenance, i32>::new();
    /// assert!(map.is_none());
    /// ```
    pub fn new() -> Option<SeparateProvenanceMap<Provenance, Value>> {
        lazy_static! {
            static ref USED_PROVENANCE: Mutex<HashSet<TypeId>> = Mutex::new(Default::default());
        }

        let used_maps: &Mutex<HashSet<TypeId>> = &*USED_PROVENANCE;
        let mut lock = used_maps.lock().unwrap();
        let used_maps = lock.deref_mut();


        let type_id = TypeId::of::<Provenance>();

        if used_maps.contains(&type_id) {
            None
        } else {
            used_maps.insert(type_id);
            Some(SeparateProvenanceMap {
                elements: vec![],
                _pd: Default::default()
            })
        }
    }

    /// Insert a value into this map.
    /// A unique key is returned. The key may be used to retrieve the value.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// let key = map.insert(5);
    /// assert_eq!(&5, map.get(key));
    /// ```
    ///
    /// Values are not required to be unique in the map.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// let key1 = map.insert(5);
    /// let key2 = map.insert(5);
    /// assert_ne!(key1, key2);
    /// ```
    pub fn insert(&mut self, value: Value) -> Key<Provenance> {
        let index = self.elements.len();
        self.elements.insert(index, value);
        Key::new(index)
    }

    /// Use a [key](Key) to retrieve an immutable reference to a stored value.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// let key = map.insert(5);
    /// assert_eq!(&5, map.get(key));
    /// ```
    pub fn get(&self, key: Key<Provenance>) -> &Value {
        // The key has the correct provenance,
        // thus we know that we created it in `insert`,
        // thus it is safe to use.
        &self.elements[key.index]
    }

    /// Use a [key](Key) to retrieve a mutable reference to a stored value.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// let key = map.insert(5);
    /// assert_eq!(&mut 5, map.get_mut(key));
    /// ```
    pub fn get_mut(&mut self, key: Key<Provenance>) -> &mut Value {
        // The key has the correct provenance,
        // thus we know that we created it in `insert`,
        // thus it is safe to use.
        &mut self.elements[key.index]
    }

    /// Get an [iterator](Iterator) over all keys in the map.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(3, map.keys().count());
    /// ```
    pub fn keys(&self) -> impl Iterator<Item = Key<Provenance>> {
        (0..self.elements.len())
            .map(|index| Key::new(index))
    }

    /// Get an [iterator](Iterator) over immutable references to each value in the map.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(6, map.iter().sum());
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.elements.iter()
    }

    /// Get an [iterator](Iterator) over mutable references to each value in the map.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// // Add one to every value
    /// map.iter_mut().for_each(|val| *val += 1);
    ///
    /// assert_eq!(9, map.iter().sum());
    /// ```
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.elements.iter_mut()
    }

    /// Search the map in insertion order for the first value that satisfy the given predicate.
    /// If such value is found, an immutable reference to it is returned,
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(Some(&2), map.find(|&val| val == 2));
    /// ```
    /// otherwise `None` is returned.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(None, map.find(|&val| val == 53));
    /// ```
    pub fn find<P: Fn(&Value) -> bool>(&self, predicate: P) -> Option<&Value> {
        for value in self.elements.iter() {
            if predicate(value) {
                return Some(value)
            }
        }

        return None
    }

    /// Search the map in insertion order for the first value that satisfy the given predicate.
    /// If such value is found, a mutable reference to it is returned,
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(Some(&mut 2), map.find_mut(|&val| val == 2));
    /// ```
    /// otherwise [`None`](std::option::Option::None) is returned.
    /// ```
    /// use provenance::SeparateProvenanceMap;
    /// struct Provenance;
    /// let mut map = SeparateProvenanceMap::<Provenance, i32>::new().unwrap();
    ///
    /// map.insert(1);
    /// map.insert(2);
    /// map.insert(3);
    ///
    /// assert_eq!(None, map.find_mut(|&val| val == 53));
    /// ```
    pub fn find_mut<P: Fn(&Value) -> bool>(&mut self, predicate: P) -> Option<&mut Value> {
        for value in self.elements.iter_mut() {
            if predicate(value) {
                return Some(value)
            }
        }

        return None
    }
}

/// A lightweight key referencing a value stored in a [ProvenanceMap](ProvenanceMap) or
/// [SeparateProvenanceMap](SeparateProvenanceMap).
///
/// Can only be created by methods on such map and thus will always be valid
/// for the map that created it. Further, the map that creates the key "tags"
/// it with it's provenance. And since there only may be one map with any given
/// provenance, it is guaranteed that if a key match the required type signature
/// for retrieving a value from a map, then that key were created by that map and
/// reference a value in that map.
pub struct Key<Provenance> {
    index: usize,
    _pd: PhantomData<*const Provenance>,
}

impl<Provenance> Key<Provenance> {
    /// Create a new key.
    ///
    /// Deliberately non-pub, since it should be created by calling methods
    /// on maps, which guarantee that the key is valid.
    fn new(index: usize) -> Self {
        Key {
            index,
            _pd: Default::default()
        }
    }
}

// Deriving traits for Key has proved unreliable, hence they are manually implemented.

impl<Provenance> Debug for Key<Provenance> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "MapKey({})", self.index)
    }
}

// Clone + Copy

impl<Provenance> Clone for Key<Provenance> {
    fn clone(&self) -> Self {
        Key {
            index: self.index,
            _pd: Default::default(),
        }
    }
}

impl<Provenance> Copy for Key<Provenance> {}

// PartialEq + Eq

impl<Provenance> PartialEq for Key<Provenance> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<Provenance> Eq for Key<Provenance> {}

// Hash

impl<Provenance> Hash for Key<Provenance> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state)
    }
}
