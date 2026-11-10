template<class I,class F,class R,bool Filter> struct RuskLazyIterator {
    Obj<I> source;Obj<F> callback;Obj<R> cached;bool ready=false;bool finished=false;
};
template<class T> void rusk_guard_iter(const Obj<T>&,Loc){}
template<class T> void rusk_guard_iter(const Obj<RuskIter<T>>& it,Loc loc){check_iter(it,loc);}
template<class I,class F,class R,bool Filter> void rusk_guard_iter(const Obj<RuskLazyIterator<I,F,R,Filter>>& it,Loc loc){if(!it->finished)rusk_guard_iter(it->source,loc);}
