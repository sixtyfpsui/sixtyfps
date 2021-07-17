/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Model and Repeater

use core::cell::RefCell;
use core::pin::Pin;
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

use crate::item_tree::TraversalOrder;
use crate::items::ItemRef;
use crate::layout::Orientation;
use crate::Property;

type ModelPeerInner = dyn ViewAbstraction;
type ComponentRc<C> = vtable::VRc<crate::component::ComponentVTable, C>;

/// Represent a handle to a view that listens to changes to a model. See [`Model::attach_peer`] and [`ModelNotify`]
#[derive(Clone)]
pub struct ModelPeer {
    inner: Weak<RefCell<ModelPeerInner>>,
}

/// Dispatch notifications from a [`Model`] to one or several [`ModelPeer`].
/// Typically, you would want to put this in the implementation of the Model
#[derive(Default)]
pub struct ModelNotify {
    inner: RefCell<weak_table::PtrWeakHashSet<Weak<RefCell<ModelPeerInner>>>>,
}

impl ModelNotify {
    /// Notify the peers that a specific row was changed
    pub fn row_changed(&self, row: usize) {
        for peer in self.inner.borrow().iter() {
            peer.borrow_mut().row_changed(row)
        }
    }
    /// Notify the peers that rows were added
    pub fn row_added(&self, index: usize, count: usize) {
        for peer in self.inner.borrow().iter() {
            peer.borrow_mut().row_added(index, count)
        }
    }
    /// Notify the peers that rows were removed
    pub fn row_removed(&self, index: usize, count: usize) {
        for peer in self.inner.borrow().iter() {
            peer.borrow_mut().row_removed(index, count)
        }
    }
    /// Attach one peer. The peer will be notified when the model changes
    pub fn attach(&self, peer: ModelPeer) {
        peer.inner.upgrade().map(|rc| self.inner.borrow_mut().insert(rc));
    }
}

/// A Model is providing Data for the Repeater or ListView elements of the `.60` language
pub trait Model {
    /// The model data: A model is a set of row and each row has this data
    type Data;
    /// The amount of row in the model
    fn row_count(&self) -> usize;
    /// Returns the data for a particular row. This function should be called with `row < row_count()`.
    fn row_data(&self, row: usize) -> Self::Data;
    /// Sets the data for a particular row. This function should be called with `row < row_count()`.
    /// If the model cannot support data changes, then it is ok to do nothing (default implementation).
    /// If the model can update the data, it should also call row_changed on its internal `ModelNotify`.
    fn set_row_data(&self, _row: usize, _data: Self::Data) {}
    /// Should forward to the internal [`ModelNotify::attach`]
    fn attach_peer(&self, peer: ModelPeer);

    /// Returns an iterator visiting all elements of the model.
    fn iter(&self) -> ModelIterator<Self::Data>
    where
        Self: Sized,
    {
        ModelIterator { model: self, row: 0 }
    }
}

/// An iterator over the elements of a model.
/// This struct is created by the [Model::iter()] trait function.
pub struct ModelIterator<'a, T> {
    model: &'a dyn Model<Data = T>,
    row: usize,
}

impl<'a, T> Iterator for ModelIterator<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row < self.model.row_count() {
            let row = self.row;
            self.row += 1;
            Some(self.model.row_data(row))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.model.row_count();
        (len, Some(len))
    }
}

impl<'a, T> ExactSizeIterator for ModelIterator<'a, T> {}

/// A model backed by a SharedVector
#[derive(Default)]
pub struct VecModel<T> {
    array: RefCell<Vec<T>>,
    notify: ModelNotify,
}

impl<T: 'static> VecModel<T> {
    /// Allocate a new model from a slice
    pub fn from_slice(slice: &[T]) -> ModelHandle<T>
    where
        T: Clone,
    {
        ModelHandle(Some(Rc::<Self>::new(slice.to_vec().into())))
    }

    /// Add a row at the end of the model
    pub fn push(&self, value: T) {
        self.array.borrow_mut().push(value);
        self.notify.row_added(self.array.borrow().len() - 1, 1)
    }

    /// Remove the row at the given index from the model
    pub fn remove(&self, index: usize) {
        self.array.borrow_mut().remove(index);
        self.notify.row_removed(index, 1)
    }
}

impl<T> From<Vec<T>> for VecModel<T> {
    fn from(array: Vec<T>) -> Self {
        VecModel { array: RefCell::new(array), notify: Default::default() }
    }
}

impl<T: Clone> Model for VecModel<T> {
    type Data = T;

    fn row_count(&self) -> usize {
        self.array.borrow().len()
    }

    fn row_data(&self, row: usize) -> Self::Data {
        self.array.borrow()[row].clone()
    }

    fn set_row_data(&self, row: usize, data: Self::Data) {
        self.array.borrow_mut()[row] = data;
        self.notify.row_changed(row);
    }

    fn attach_peer(&self, peer: ModelPeer) {
        self.notify.attach(peer);
    }
}

impl Model for usize {
    type Data = i32;

    fn row_count(&self) -> usize {
        *self
    }

    fn row_data(&self, row: usize) -> Self::Data {
        row as i32
    }

    fn attach_peer(&self, _peer: ModelPeer) {
        // The model is read_only: nothing to do
    }
}

impl Model for bool {
    type Data = ();

    fn row_count(&self) -> usize {
        if *self {
            1
        } else {
            0
        }
    }

    fn row_data(&self, _row: usize) -> Self::Data {}

    fn attach_peer(&self, _peer: ModelPeer) {
        // The model is read_only: nothing to do
    }
}

/// Properties of type array in the .60 language are represented as
/// an [`Option`] of an [`Rc`] of something implemented the [`Model`] trait
#[derive(derive_more::Deref, derive_more::DerefMut, derive_more::From, derive_more::Into)]
pub struct ModelHandle<T>(pub Option<Rc<dyn Model<Data = T>>>);

impl<T> std::fmt::Debug for ModelHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ModelHandle({:?})", self.0.as_ref().map(|_| "dyn Model"))
    }
}

impl<T> Clone for ModelHandle<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Default for ModelHandle<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> core::cmp::PartialEq for ModelHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some(_), None) => false,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        }
    }
}

impl<T> From<Rc<dyn Model<Data = T>>> for ModelHandle<T> {
    fn from(x: Rc<dyn Model<Data = T>>) -> Self {
        Self(Some(x))
    }
}

impl<T> ModelHandle<T> {
    /// Create a new handle wrapping the given model
    pub fn new(model: Rc<dyn Model<Data = T>>) -> Self {
        Self(Some(model))
    }
}

impl<T> Model for ModelHandle<T> {
    type Data = T;

    fn row_count(&self) -> usize {
        self.0.as_ref().map_or(0, |model| model.row_count())
    }

    fn row_data(&self, row: usize) -> Self::Data {
        self.0.as_ref().unwrap().row_data(row)
    }

    fn attach_peer(&self, peer: ModelPeer) {
        if let Some(model) = self.0.as_ref() {
            model.attach_peer(peer);
        }
    }
}

/// Component that can be instantiated by a repeater.
pub trait RepeatedComponent: crate::component::Component {
    /// The data corresponding to the model
    type Data: 'static;

    /// Update this component at the given index and the given data
    fn update(&self, index: usize, data: Self::Data);

    /// Layout this item in the listview
    ///
    /// offset_y is the `y` position where this item should be placed.
    /// it should be updated to be to the y position of the next item.
    fn listview_layout(
        self: Pin<&Self>,
        _offset_y: &mut f32,
        _viewport_width: Pin<&Property<f32>>,
    ) {
    }

    /// Returns what's needed to perform the layout if this component is in a box layout
    fn box_layout_data(
        self: Pin<&Self>,
        _orientation: Orientation,
    ) -> crate::layout::BoxLayoutCellData {
        crate::layout::BoxLayoutCellData::default()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum RepeatedComponentState {
    /// The item is in a clean state
    Clean,
    /// The model data is stale and needs to be refreshed
    Dirty,
}
struct RepeaterInner<C: RepeatedComponent> {
    components: Vec<(RepeatedComponentState, Option<ComponentRc<C>>)>,
    is_dirty: Pin<Box<Property<bool>>>,
    /// The model row (index) of the first component in the `components` vector.
    /// Only used for ListView
    offset: usize,
    /// The average visible item_height. Only used for ListView
    cached_item_height: f32,
}

impl<C: RepeatedComponent> Default for RepeaterInner<C> {
    fn default() -> Self {
        RepeaterInner {
            components: Default::default(),
            is_dirty: Box::pin(Property::new(true)),
            offset: 0,
            cached_item_height: 0.,
        }
    }
}

impl<C: RepeatedComponent> Clone for RepeaterInner<C> {
    fn clone(&self) -> Self {
        panic!("Clone is there so we can make_mut the RepeaterInner, to dissociate the weak refs, but there should only be one inner")
    }
}

trait ViewAbstraction {
    fn row_changed(&mut self, row: usize);
    fn row_added(&mut self, index: usize, count: usize);
    fn row_removed(&mut self, index: usize, count: usize);
}

impl<C: RepeatedComponent> ViewAbstraction for RepeaterInner<C> {
    /// Notify the peers that a specific row was changed
    fn row_changed(&mut self, row: usize) {
        self.is_dirty.as_ref().set(true);
        if let Some(c) = self.components.get_mut(row.wrapping_sub(self.offset)) {
            c.0 = RepeatedComponentState::Dirty;
        }
    }
    /// Notify the peers that rows were added
    fn row_added(&mut self, mut index: usize, mut count: usize) {
        if index < self.offset {
            if index + count < self.offset {
                return;
            }
            count -= self.offset - index;
            index = 0;
        } else {
            index -= self.offset;
        }
        if count == 0 || index > self.components.len() {
            return;
        }
        self.is_dirty.as_ref().set(true);
        self.components.splice(
            index..index,
            core::iter::repeat((RepeatedComponentState::Dirty, None)).take(count),
        );
    }
    /// Notify the peers that rows were removed
    fn row_removed(&mut self, mut index: usize, mut count: usize) {
        if index < self.offset {
            if index + count < self.offset {
                return;
            }
            count -= self.offset - index;
            index = 0;
        } else {
            index -= self.offset;
        }
        if count == 0 || index >= self.components.len() {
            return;
        }
        if (index + count) > self.components.len() {
            count = self.components.len() - index;
        }
        self.is_dirty.as_ref().set(true);
        self.components.drain(index..(index + count));
        for c in self.components[index..].iter_mut() {
            // Because all the indexes are dirty
            c.0 = RepeatedComponentState::Dirty;
        }
    }
}

/// This field is put in a component when using the `for` syntax
/// It helps instantiating the components `C`
pub struct Repeater<C: RepeatedComponent> {
    /// The Rc is shared between ModelPeer. The outer RefCell make it possible to re-initialize a new Rc when
    /// The model is changed. The inner RefCell make it possible to change the RepeaterInner when shared
    inner: RefCell<Rc<RefCell<RepeaterInner<C>>>>,
    model: Property<ModelHandle<C::Data>>,
    /// Only used for the list view to track if the scrollbar has changed and item needs to be layed out again.
    listview_geometry_tracker: crate::properties::PropertyTracker,
}

impl<C: RepeatedComponent> Default for Repeater<C> {
    fn default() -> Self {
        Repeater {
            inner: Default::default(),
            model: Default::default(),
            listview_geometry_tracker: Default::default(),
        }
    }
}

impl<C: RepeatedComponent + 'static> Repeater<C> {
    fn model(self: Pin<&Self>) -> ModelHandle<C::Data> {
        // Safety: Repeater does not implement drop and never let access model as mutable
        #[allow(unsafe_code)]
        let model = unsafe { self.map_unchecked(|s| &s.model) };

        if model.is_dirty() {
            // Invalidate previous weeks on the previous models
            (*Rc::make_mut(&mut self.inner.borrow_mut()).get_mut()) = RepeaterInner::default();
            if let ModelHandle(Some(m)) = model.get() {
                let peer: Rc<RefCell<dyn ViewAbstraction>> = self.inner.borrow().clone();
                m.attach_peer(ModelPeer { inner: Rc::downgrade(&peer) });
            }
        }
        model.get()
    }

    /// Call this function to make sure that the model is updated.
    /// The init function is the function to create a component
    pub fn ensure_updated(self: Pin<&Self>, init: impl Fn() -> ComponentRc<C>) {
        if let ModelHandle(Some(model)) = self.model() {
            if self.inner.borrow().borrow().is_dirty.as_ref().get() {
                self.ensure_updated_impl(init, &model, model.row_count());
            }
        } else {
            self.inner.borrow().borrow_mut().components.clear();
        }
    }

    // returns true if new items were created
    fn ensure_updated_impl(
        self: Pin<&Self>,
        init: impl Fn() -> ComponentRc<C>,
        model: &Rc<dyn Model<Data = C::Data>>,
        count: usize,
    ) -> bool {
        let inner = self.inner.borrow();
        let mut inner = inner.borrow_mut();
        inner.components.resize_with(count, || (RepeatedComponentState::Dirty, None));
        let offset = inner.offset;
        let mut created = false;
        for (i, c) in inner.components.iter_mut().enumerate() {
            if c.0 == RepeatedComponentState::Dirty {
                if c.1.is_none() {
                    created = true;
                    c.1 = Some(init());
                }
                c.1.as_ref().unwrap().update(i + offset, model.row_data(i + offset));
                c.0 = RepeatedComponentState::Clean;
            }
        }
        inner.is_dirty.as_ref().set(false);
        created
    }

    /// Same as `Self::ensuer_updated` but for a ListView
    pub fn ensure_updated_listview(
        self: Pin<&Self>,
        init: impl Fn() -> ComponentRc<C>,
        viewport_width: Pin<&Property<f32>>,
        viewport_height: Pin<&Property<f32>>,
        viewport_y: Pin<&Property<f32>>,
        listview_width: f32,
        listview_height: Pin<&Property<f32>>,
    ) {
        let empty_model = || {
            self.inner.borrow().borrow_mut().components.clear();
            viewport_height.set(0.);
            viewport_y.set(0.);
        };

        let model = if let ModelHandle(Some(model)) = self.model() {
            model
        } else {
            return empty_model();
        };
        let row_count = model.row_count();
        if row_count == 0 {
            return empty_model();
        }

        #[allow(unsafe_code)]
        // Safety: Repeater does not implement drop and never let access model as mutable
        let listview_geometry_tracker =
            unsafe { self.map_unchecked(|s| &s.listview_geometry_tracker) };
        let init = &init;
        if listview_geometry_tracker
            .evaluate_if_dirty(|| {
                let listview_height = listview_height.get();
                // Compute the element height
                let total_height = Cell::new(0.);
                let min_height = Cell::new(listview_height);
                let count = Cell::new(0);

                let get_height_visitor = |item: Pin<ItemRef>| {
                    count.set(count.get() + 1);
                    let height = item.as_ref().geometry().height();
                    total_height.set(total_height.get() + height);
                    min_height.set(min_height.get().min(height))
                };
                for c in self.inner.borrow().borrow().components.iter() {
                    if let Some(x) = c.1.as_ref() {
                        get_height_visitor(x.as_pin_ref().get_item_ref(0));
                    }
                }

                let element_height = if count.get() > 0 {
                    total_height.get() / (count.get() as f32)
                } else {
                    // There seems to be currently no items. Just instantiate one item.

                    {
                        let inner = self.inner.borrow();
                        let mut inner = inner.borrow_mut();
                        inner.offset = inner.offset.min(row_count - 1);
                    }

                    self.ensure_updated_impl(&init, &model, 1);
                    if let Some(c) = self.inner.borrow().borrow().components.get(0) {
                        if let Some(x) = c.1.as_ref() {
                            get_height_visitor(x.as_pin_ref().get_item_ref(0));
                        }
                    } else {
                        panic!("Could not determine size of items");
                    }
                    total_height.get()
                };

                let min_height = min_height.get().min(element_height).max(1.);

                viewport_height.set(element_height * row_count as f32);
                self.inner.borrow().borrow_mut().cached_item_height = element_height;
                if -viewport_y.get() > element_height * row_count as f32 - listview_height {
                    viewport_y.set(-(element_height * row_count as f32 - listview_height).max(0.))
                }
                let offset = (-viewport_y.get_untracked() / element_height).floor() as usize;
                let mut count =
                    ((listview_height / min_height).ceil() as usize).min(row_count - offset);
                loop {
                    self.set_offset(offset, count);
                    self.ensure_updated_impl(init, &model, count);
                    let end = self.compute_layout_listview(viewport_width, listview_width);
                    let diff = listview_height - viewport_y.get_untracked() - end;
                    if diff > 0. && count < row_count - offset {
                        count = (count + (diff / element_height).ceil() as usize)
                            .min(row_count - offset);
                        continue;
                    }
                    break;
                }
            })
            .is_none()
        {
            if self.inner.borrow().borrow().is_dirty.as_ref().get() {
                let count = self
                    .inner
                    .borrow()
                    .borrow()
                    .components
                    .len()
                    .min(row_count.saturating_sub(self.inner.borrow().borrow().offset));
                self.ensure_updated_impl(init, &model, count);
                self.compute_layout_listview(viewport_width, listview_width);
            }
        }
    }

    fn set_offset(&self, offset: usize, count: usize) {
        let inner = self.inner.borrow();
        let mut inner = inner.borrow_mut();
        let old_offset = inner.offset;
        // Remove the items before the offset, or add items until the old offset
        let to_remove = offset.saturating_sub(old_offset);
        if to_remove < inner.components.len() {
            inner.components.splice(
                0..to_remove,
                core::iter::repeat((RepeatedComponentState::Dirty, None))
                    .take(old_offset.saturating_sub(offset)),
            );
        } else {
            inner.components.truncate(0);
        }
        inner.components.resize_with(count, || (RepeatedComponentState::Dirty, None));
        inner.offset = offset;
        inner.is_dirty.as_ref().set(true);
    }

    /// Sets the data directly in the model
    pub fn model_set_row_data(self: Pin<&Self>, row: usize, data: C::Data) {
        if let ModelHandle(Some(model)) = self.model() {
            model.set_row_data(row, data);
            if let Some(c) = self.inner.borrow().borrow_mut().components.get_mut(row) {
                if c.0 == RepeatedComponentState::Dirty {
                    if let Some(comp) = c.1.as_ref() {
                        comp.update(row, model.row_data(row));
                        c.0 = RepeatedComponentState::Clean;
                    }
                }
            }
        }
    }
}

impl<C: RepeatedComponent> Repeater<C> {
    /// Set the model binding
    pub fn set_model_binding(&self, binding: impl Fn() -> ModelHandle<C::Data> + 'static) {
        self.model.set_binding(binding);
    }

    /// Call the visitor for each component
    pub fn visit(
        &self,
        order: TraversalOrder,
        mut visitor: crate::item_tree::ItemVisitorRefMut,
    ) -> crate::item_tree::VisitChildrenResult {
        // We can't keep self.inner borrowed because the event might modify the model
        let count = self.inner.borrow().borrow().components.len();
        for i in 0..count {
            let i = if order == TraversalOrder::BackToFront { i } else { count - i - 1 };
            let c = self.inner.borrow().borrow().components.get(i).and_then(|c| c.1.clone());
            if let Some(c) = c {
                if c.as_pin_ref().visit_children_item(-1, order, visitor.borrow_mut()).has_aborted()
                {
                    return crate::item_tree::VisitChildrenResult::abort(i, 0);
                }
            }
        }
        crate::item_tree::VisitChildrenResult::CONTINUE
    }

    /// Return the amount of item currently in the component
    pub fn len(&self) -> usize {
        self.inner.borrow().borrow().components.len()
    }

    /// Returns a vector containing all components
    pub fn components_vec(&self) -> Vec<ComponentRc<C>> {
        self.inner.borrow().borrow().components.iter().flat_map(|x| x.1.clone()).collect()
    }

    /// Set the position of all the element in the listview
    ///
    /// Returns the offset of the end of the last element
    pub fn compute_layout_listview(
        &self,
        viewport_width: Pin<&Property<f32>>,
        listview_width: f32,
    ) -> f32 {
        let inner = self.inner.borrow();
        let inner = inner.borrow();
        let mut y_offset = inner.offset as f32 * inner.cached_item_height;
        viewport_width.set(listview_width);
        for c in self.inner.borrow().borrow().components.iter() {
            if let Some(x) = c.1.as_ref() {
                x.as_pin_ref().listview_layout(&mut y_offset, viewport_width);
            }
        }
        y_offset
    }
}

/// Represent an item in a StandardListView
#[repr(C)]
#[derive(Clone, Default, Debug, PartialEq)]
pub struct StandardListViewItem {
    /// The text content of the item
    pub text: crate::SharedString,
}
