#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

pub struct Node { pub val: u64, }

pub struct Pool {
    len: u64, __eyk_erased_nodes: Tracked < Region < Ptr < Node >>>, __eyk_erased_order: Ghost < Seq < Ptr < Node >>>,
}

impl Pool {
    pub closed spec fn invariant(&self) -> bool {
        (forall | q | self.__eyk_erased_order@.contains(q) ==> self.__eyk_erased_nodes@.contains(q))
    }

}

impl Pool {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn find_zero(&mut self, p: Ptr < Node >, n: u64)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        let mut i = 0;
        while i < n
        invariant i <= n, self.__eyk_erased_order@.contains(p), self.invariant(),
        decreases n - i, {
            {
                let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
                let __eyk_res = {
                    if (*p.read(Tracked(&__eyk_perm_p))).val == 0 {
                        {
                            proof {
                                self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                            }
                            break
                        };
                    }
                };
                #[allow(unreachable_code)] let __eyk_res = {
                    proof {
                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                    }
                    __eyk_res
                };
                __eyk_res
            }
            i = i + 1;
        }
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn find_zero_leaked(&mut self, p: Ptr < Node >, n: u64)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        let mut i = 0;
        while i < n
        invariant i <= n, self.__eyk_erased_order@.contains(p), self.invariant(),
        decreases n - i, {
            let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
            if (*p.read(Tracked(&__eyk_perm_p))).val == 0 {
                break;
            }
            proof {
                self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
            }
            i = i + 1;
        }
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn skip_zero(&mut self, p: Ptr < Node >, n: u64)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        let mut i = 0;
        while i < n
        invariant i <= n, self.__eyk_erased_order@.contains(p), self.invariant(),
        decreases n - i, {
            i = i + 1;
            {
                let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
                let __eyk_res = {
                    if (*p.read(Tracked(&__eyk_perm_p))).val == 0 {
                        {
                            proof {
                                self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                            }
                            continue
                        };
                    }
                    p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = (*p.read(Tracked(&__eyk_perm_p))).val - 1;
                };
                #[allow(unreachable_code)] let __eyk_res = {
                    proof {
                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                    }
                    __eyk_res
                };
                __eyk_res
            }
        }
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn big_or_zero(&mut self, p: Ptr < Node >) -> u64
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        {
            let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
            let __eyk_res = {
                if (*p.read(Tracked(&__eyk_perm_p))).val > 10 {
                    {
                        let __eyk_exit = (*p.read(Tracked(&__eyk_perm_p))).val;
                        proof {
                            self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                        }
                        return __eyk_exit
                    };
                }
                p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 0;
            };
            #[allow(unreachable_code)] let __eyk_res = {
                proof {
                    self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                }
                __eyk_res
            };
            __eyk_res
        }
        0
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn set_some(&mut self, p: Ptr < Node >, o: Option < u64 >) -> Option < u64 >
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        {
            let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
            let __eyk_res = {
                p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = match o {
                    Some(__eyk_ok) => __eyk_ok, None => {
                        proof {
                            self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                        }
                        return None
                    }
                };
            };
            #[allow(unreachable_code)] let __eyk_res = {
                proof {
                    self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                }
                __eyk_res
            };
            __eyk_res
        }
        Some(0)
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn set_ok(&mut self, p: Ptr < Node >, o: Result < u64, u8 >) -> Result < u64, u8 >
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        {
            let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
            let __eyk_res = {
                p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = match o {
                    Ok(__eyk_ok) => __eyk_ok, Err(__eyk_err) => {
                        proof {
                            self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                        }
                        return Err(::core::convert::From::from(__eyk_err))
                    }
                };
            };
            #[allow(unreachable_code)] let __eyk_res = {
                proof {
                    self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                }
                __eyk_res
            };
            __eyk_res
        }
        Ok(0)
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn short_circuit(&mut self, p: Ptr < Node >, c: bool)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        {
            let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
            let __eyk_res = {
                let _b = c && {
                    {
                        proof {
                            self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                        }
                        return
                    };
                };
            };
            #[allow(unreachable_code)] let __eyk_res = {
                proof {
                    self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                }
                __eyk_res
            };
            __eyk_res
        }
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn nested(&mut self, p: Ptr < Node >, q: Ptr < Node >, n: u64)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p), old(self).__eyk_erased_order@.contains(q), p != q,
    ensures final(self).invariant(), {
        let mut i = 0;
        while i < n
        invariant i <= n, self.__eyk_erased_order@.contains(p), self.__eyk_erased_order@.contains(q), p != q, self.invariant(),
        decreases n - i, {
            {
                let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
                let __eyk_res = {
                    {
                        let tracked __eyk_perm_q = self.__eyk_erased_nodes.borrow_mut().remove(q);
                        let __eyk_res = {
                            if (*p.read(Tracked(&__eyk_perm_p))).val == (*q.read(Tracked(&__eyk_perm_q))).val {
                                {
                                    proof {
                                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_q);
                                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                                    }
                                    break
                                };
                            }
                        };
                        #[allow(unreachable_code)] let __eyk_res = {
                            proof {
                                self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_q);
                            }
                            __eyk_res
                        };
                        __eyk_res
                    }
                };
                #[allow(unreachable_code)] let __eyk_res = {
                    proof {
                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                    }
                    __eyk_res
                };
                __eyk_res
            }
            i = i + 1;
        }
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn inner_loop(&mut self, p: Ptr < Node >)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        {
            let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
            let __eyk_res = {
                let v = (*p.read(Tracked(&__eyk_perm_p))).val;
                let mut k: u64 = 0;
                while k < 5
                invariant k <= 5,
                decreases 5 - k, {
                    if k == v {
                        break;
                    }
                    k = k + 1;
                }
                p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = k;
            };
            #[allow(unreachable_code)] let __eyk_res = {
                proof {
                    self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                }
                __eyk_res
            };
            __eyk_res
        }
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn labeled(&mut self, p: Ptr < Node >) -> u64
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        let v = 'a:{
            {
                let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
                let __eyk_res = {
                    'b:{
                        if (*p.read(Tracked(&__eyk_perm_p))).val == 1 {
                            break 'b;
                        }
                        p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 2;
                    }
                    if (*p.read(Tracked(&__eyk_perm_p))).val > 5 {
                        {
                            let __eyk_exit = (*p.read(Tracked(&__eyk_perm_p))).val;
                            proof {
                                self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                            }
                            break 'a __eyk_exit
                        };
                    }
                };
                #[allow(unreachable_code)] let __eyk_res = {
                    proof {
                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                    }
                    __eyk_res
                };
                __eyk_res
            }
            0
        };
        v
    }

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn loop_then_take(&mut self, p: Ptr < Node >)
    requires old(self).invariant(), old(self).__eyk_erased_order@.contains(p),
    ensures final(self).invariant(), {
        loop
        invariant self.__eyk_erased_order@.contains(p), self.invariant(),
        decreases 1int, {
            {
                let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
                let __eyk_res = {
                    {
                        proof {
                            self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                        }
                        break
                    };
                };
                #[allow(unreachable_code)] let __eyk_res = {
                    proof {
                        self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
                    }
                    __eyk_res
                };
                __eyk_res
            }
        }
        let tracked __eyk_perm_p = self.__eyk_erased_nodes.borrow_mut().remove(p);
        p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 1;
        proof {
            self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_p);
        }
    }

}

} // verus!
