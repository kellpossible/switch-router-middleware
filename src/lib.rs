use log::error;
use reactive_state::{
    middleware::{Middleware, ReduceFn},
    Store, StoreEvent, StoreRef,
};
use std::{
    cell::RefCell,
    fmt::{Debug, Display},
    hash::Hash,
    marker::PhantomData,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use switch_router::{SwitchRouteService, SwitchRoute};

pub struct RouteMiddleware<R, RS, State, Action, Event, Effect> {
    pub route_service: RefCell<RS>,
    /// The callback to the SwitchRouteService. When this gets dropped
    /// this listener will be removed from the route service.
    _callback: switch_router::Callback<R>,
    state_type: PhantomData<State>,
    action_type: PhantomData<Action>,
    event_type: PhantomData<Event>,
    effect_type: PhantomData<Effect>,
}

impl<R, RS, State, Action, Event, Effect> RouteMiddleware<R, RS, State, Action, Event, Effect>
where
    R: SwitchRoute + 'static,
    RS: SwitchRouteService<Route = R> + 'static,
    State: 'static,
    Action: IsRouteAction<R> + 'static,
    Event: Clone + Hash + Eq + StoreEvent + 'static,
    Effect: 'static,
{
    pub fn new(route_service: RS, store: StoreRef<State, Action, Event, Effect>) -> Self {
        let router = RefCell::new(route_service);
        let callback: switch_router::Callback<R> =
            switch_router::Callback::new(move |route: R| {
                store.dispatch(RouteAction::BrowserChangeRoute(route));
            });

        // FIXME: there is multiple borrow error with this callback
        match router.try_borrow_mut() {
            Ok(mut router_mut) => {
                router_mut.register_callback(&callback);
            }
            Err(err) => {
                error!("Unable to register callback {:?}: {}", callback, err);
            }
        }

        Self {
            route_service: router,
            _callback: callback,
            state_type: PhantomData,
            action_type: PhantomData,
            event_type: PhantomData,
            effect_type: PhantomData,
        }
    }

    fn set_route<SRI: Into<R>>(&self, switch_route: SRI) {
        match self.route_service.try_borrow_mut() {
            Ok(mut router) => {
                router.set_route(switch_route);
            }
            Err(err) => {
                error!("Unable to borrow route_service for RouteMiddleware: {}", err);
            }
        }
    }

    fn back(&self) -> Option<R> {
        match self.route_service.try_borrow_mut() {
            Ok(mut router) => {
                router.back()
            }
            Err(err) => {
                error!("Unable to borrow route_service for RouteMiddleware: {}", err);
                None
            }
        }
    }
}

impl<R, RS, State, Action, Event, Effect> Middleware<State, Action, Event, Effect>
    for RouteMiddleware<R, RS, State, Action, Event, Effect>
where
    R: SwitchRoute + 'static,
    RS: SwitchRouteService<Route = R> + 'static,
    Action: IsRouteAction<R> + Debug + 'static,
    State: RouteState<R> + 'static,
    Event: RouteEvent<R> + PartialEq + Clone + Hash + Eq + StoreEvent + 'static,
    Effect: 'static,
{
    fn on_reduce(
        &self,
        store: &Store<State, Action, Event, Effect>,
        action: Option<&Action>,
        reduce: ReduceFn<State, Action, Event, Effect>,
    ) -> reactive_state::middleware::ReduceMiddlewareResult<Event, Effect> {
        if let Some(action) = &action {
            if let Some(route_action) = action.route_action() {
                match route_action {
                    RouteAction::Back => {
                        self.back();
                        return reduce(
                            store,
                            None,
                        );
                    }
                    RouteAction::ChangeRoute(route) => {
                        self.set_route(route.clone());
                    }
                    RouteAction::PollBrowserRoute => match self.route_service.try_borrow_mut() {
                        Ok(router_mut) => {
                            let route = router_mut.get_route();
                            return reduce(
                                store,
                                Some(&RouteAction::BrowserChangeRoute(route).into()),
                            );
                        }
                        Err(err) => {
                            error!("Cannot borrow mut self.router: {}", err);
                        }
                    },
                    _ => {}
                }
            }
        }
        reduce(store, action)
    }
}

pub trait RouteState<SR> {
    fn get_route(&self) -> &SR;
}

pub trait RouteEvent<SR>
where
    SR: SwitchRoute + 'static,
{
    fn route_changed() -> Self;
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, PartialEq, Clone)]
pub enum RouteAction<SR> {
    Back,
    ChangeRoute(SR),
    BrowserChangeRoute(SR),
    PollBrowserRoute,
}

impl<SR> Display for RouteAction<SR>
where
    SR: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteAction::Back => write!(f, "Back"),
            RouteAction::ChangeRoute(route) => write!(f, "ChangeRoute({:?})", route),
            RouteAction::BrowserChangeRoute(route) => write!(f, "BrowserChangeRoute({:?})", route),
            RouteAction::PollBrowserRoute => write!(f, "PollBrowserRoute"),
        }
    }
}

pub trait IsRouteAction<SR>: Clone + From<RouteAction<SR>>
where
    SR: SwitchRoute + 'static,
{
    fn route_action(&self) -> Option<&RouteAction<SR>>;
}

pub trait RouteStore<SR> {
    fn change_route<R: Into<SR>>(&self, route: R);
}

impl<SR, State, Action, Event, Effect> RouteStore<SR> for Store<State, Action, Event, Effect>
where
    SR: SwitchRoute + 'static,
    Action: IsRouteAction<SR>,
    State: RouteState<SR>,
    Event: RouteEvent<SR> + PartialEq + StoreEvent + Clone + Hash + Eq,
{
    fn change_route<R: Into<SR>>(&self, route: R) {
        self.dispatch(RouteAction::ChangeRoute(route.into()));
    }
}
