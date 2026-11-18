import {expect,it,vi} from 'vitest'
import {createClient,type Transport} from '../src/http/client'
import {createIdentityClient} from '../src/modules/identity'
const headers={'content-type':'application/vnd.api+json'}
const session=(csrf:string)=>({status:200,headers,body:{data:{type:'sessions',id:'session',attributes:{csrf,user:{id:'admin'}}}}})
it('identity factory rotates CSRF across login, session, writes and logout and isolates transports',async()=>{
 const transport=vi.fn<Transport>().mockResolvedValue(session('login-token'))
 const otherTransport=vi.fn<Transport>().mockResolvedValue({status:200,headers,body:{data:null}})
 const identity=createIdentityClient(createClient(transport)),other=createIdentityClient(createClient(otherTransport))
 await identity.login('admin','password')
 transport.mockResolvedValueOnce(session('session-token'))
 await identity.session()
 await identity.profile('新姓名')
 expect(transport.mock.calls[1][0].headers['X-CSRF-Token']).toBe('login-token')
 expect(transport.mock.calls[2][0].headers['X-CSRF-Token']).toBe('session-token')
 await other.profile('独立用户')
 expect(otherTransport.mock.calls[0][0].headers).not.toHaveProperty('X-CSRF-Token')
 transport.mockResolvedValue({status:200,headers,body:{data:null}})
 await identity.logout();transport.mockResolvedValueOnce(session('next'));await identity.session()
 expect(transport.mock.calls[3][0].headers['X-CSRF-Token']).toBe('session-token')
 expect(transport.mock.calls[4][0].headers).not.toHaveProperty('X-CSRF-Token')
})
it('failed logout retains the token until session is rechecked',async()=>{
 const transport=vi.fn<Transport>().mockResolvedValue(session('original'))
 const identity=createIdentityClient(createClient(transport))
 await identity.login('admin','password')
 transport.mockRejectedValueOnce(new Error('network'))
 await expect(identity.logout()).rejects.toMatchObject({code:'network_error'})
 await identity.session()
 expect(transport.mock.calls[2][0].headers['X-CSRF-Token']).toBe('original')
})
