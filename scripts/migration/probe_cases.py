"""Observe original DNS/TLS grades with only the browser observation substituted."""
from __future__ import annotations
from numera.probe.dns import _grade_dns, _DNS_JS
from numera.probe.network import check_tls, _TLS_PARSE
from numera.models import Profile
from numera.fingerprint import generate


def cases():
    rows = []
    dns_inputs = [([], None), ([{'type':'ip','ip':'192.0.2.1'}], 'US'),
                  ([{'type':'dns','ip':'192.0.2.2','country_code':'US','country':'United States','asn':123}], 'us'),
                  ([{'type':'DNS','ip':'192.0.2.3','country_code':'DE','country':'Germany','asn':'AS2'},
                    {'type':'dns','ip':'192.0.2.4','country_code':'US','country':'United States','asn':'AS1'}], 'US'),
                  ([{'type':'dns','country':'Germany','asn':0}], 'US'),
                  ([{'type':'dns','country_code':'DE','asn':12}], None)]
    for observations, country in dns_inputs:
        rows.append({'operation':'dns','observation':{'rows':observations},'country':country,
                     'expected':_grade_dns(observations,country)})
    class Page:
        def __init__(self, observation): self.observation=observation
        def goto(self,*args,**kwargs): pass
        def evaluate(self,*args): return self.observation
    class Context:
        def __init__(self, observation): self.pages=[Page(observation)]
    tls_inputs = [None, {'ja3':'771,1-2-3,0,0,0','ja3Hash':'fixture','ja4':None,'http2':None,'grease':False,'greaseKnown':False,'cipherCount':3},
                  {'ja3':None,'ja3Hash':None,'ja4':'t13d1516h2_fixture','http2':'fixture','grease':True,'greaseKnown':True,'cipherCount':15},
                  {'ja3':None,'ja3Hash':'fixture','ja4':'t12d0303h1_fixture','http2':None,'grease':False,'greaseKnown':True,'cipherCount':4}]
    for observation in tls_inputs:
        rows.append({'operation':'tls','observation':observation,'expected':check_tls(Profile(name='Fixture',fingerprint=generate(seed=42)),None,context=Context(observation))})
    usable = tls_inputs[2]
    retries = []
    for attempts in [
        [{'navigate':False},{'evaluate':False},{'value':usable}],
        [{'value':None},{'value':usable}],
        [{'value':None},{'value':None},{'value':None}],
    ]:
        class RetryPage:
            def __init__(self): self.calls=[]; self.index=-1
            def goto(self,url,**kwargs):
                self.index+=1
                self.calls.append({'url':url,'timeout':kwargs['timeout']})
                if attempts[self.index].get('navigate') is False:
                    raise RuntimeError('Synthetic SDK navigation failure')
            def evaluate(self,*args):
                self.calls[-1]['evaluated']=True
                if attempts[self.index].get('evaluate') is False:
                    raise RuntimeError('Synthetic SDK evaluation failure')
                return attempts[self.index].get('value')
        page = RetryPage()
        context = type('RetryContext',(),{'pages':[page]})()
        expected = check_tls(Profile(name='Fixture',fingerprint=generate(seed=42)),None,context=context)
        retries.append({'attempts':attempts,'expected':expected,'calls':page.calls})
    return {'cases':rows,'retries':retries,'tlsScript':_TLS_PARSE.strip(),'dnsScript':_DNS_JS.strip(),
            'network':network_cases(),'trust':trust_cases()}


def network_cases():
    from contextlib import contextmanager
    from unittest.mock import patch
    from numera.models import Proxy
    from numera.probe.network import check_proxy, _IP_JS, _WEBRTC_JS, _is_private
    rows = []
    healthy_tls = {'ja3':None,'ja3Hash':'fixture','ja4':'t13d1516h2_fixture',
                   'http2':None,'grease':True,'greaseKnown':True,'cipherCount':15}
    for info, ips, country, timezone in [
        ({'ip':'203.0.113.1','country':'DE','city':'Berlin','timezone':'Europe/Berlin'},
         ['192.168.1.1','203.0.113.1'], 'DE', 'Europe/Berlin'),
        ({'ip':'203.0.113.1','country':'DE','city':None,'timezone':'Asia/Calcutta'},
         ['203.0.113.2','::1','fd00::1'], 'US', 'Asia/Kolkata'),
        ({'ip':'203.0.113.1','country':None,'city':None,'timezone':None}, [], None, 'Europe/Berlin'),
        (None, ['198.51.100.1'], None, 'Europe/Berlin'),
    ]:
        profile = Profile(name='Fixture',fingerprint=generate(seed=42),
                          proxy=Proxy('http://proxy.fixture.test:8080',country=country) if country else None)
        profile.fingerprint.timezone = timezone
        seen = []
        dns = {'rows':[{'type':'dns','ip':'192.0.2.1','country_code':'DE','asn':'AS1'}]}
        ice = {'supported':True,'ips':ips}
        class NetworkPage:
            def goto(self,url,**kwargs): seen.append({'operation':'navigate','url':url})
            def evaluate(self,expression,argument=None):
                if expression == _IP_JS: kind,value='ip',info
                elif expression == _WEBRTC_JS: kind,value='ice',ice
                elif expression == _DNS_JS: kind,value='dns',dns
                elif expression == _TLS_PARSE: kind,value='tls',healthy_tls
                else: raise AssertionError('Unknown original probe script')
                seen.append({'operation':'evaluate','kind':kind,'argument':argument})
                return value
        @contextmanager
        def session(*args,**kwargs):
            seen.append({'operation':'open'})
            try: yield type('NetworkContext',(),{'pages':[NetworkPage()]})()
            finally: seen.append({'operation':'close'})
        with patch('numera.probe.network.session',session), patch('numera.probe.network.time.time',side_effect=[1000,1000.125]):
            expected = check_proxy(profile,None)
        rows.append({'profile':profile.to_dict(),'observations':{'info':info,'ice':ice,'dns':dns,'tls':healthy_tls},
                     'expected':expected,'calls':seen})
    profile = Profile(name='Fixture',fingerprint=generate(seed=42))
    failed_calls = []
    class FailedPage:
        def evaluate(self,*args):
            failed_calls.append({'operation':'evaluate','kind':'ip'})
            raise RuntimeError('Synthetic IP SDK failure')
        def goto(self,*args,**kwargs): raise AssertionError('IP failure continued to TLS')
    @contextmanager
    def failed_session(*args,**kwargs):
        failed_calls.append({'operation':'open'})
        try: yield type('FailedContext',(),{'pages':[FailedPage()]})()
        finally: failed_calls.append({'operation':'close'})
    with patch('numera.probe.network.session',failed_session):
        failed = check_proxy(profile,None)
    rows.append({'profile':profile.to_dict(),'observations':{'ipFailed':True},
                 'expected':failed,'calls':failed_calls})
    addresses = ['10.0.0.1','127.0.0.1','172.15.0.1','172.16.0.1','172.31.0.1','172.32.0.1',
                 '192.168.1.1','169.254.1.1','203.0.113.1','999.1.1.1','invalid','١٠.٠.٠.١',
                 'fc00::1','fd00::1','fe80::1','feb0::1','fec0::1','::1','::123','2001:db8::1']
    return {'cases':rows,'addresses':[{'ip':ip,'private':_is_private(ip)} for ip in addresses],
            'ipScript':_IP_JS.strip(),'iceScript':_WEBRTC_JS.strip()}


def trust_cases():
    from numera.probe.trust import _grade, _TRUST_JS
    from numera.probe.shared import PROBE_URL, PROBE_HTML
    profile = Profile(name='Fixture',fingerprint=generate(seed=42))
    fp = profile.fingerprint
    seen = {'webdriver':False,'webdriverOwn':False,'hasChrome':True,'plugins':5,
            'userAgent':fp.user_agent,'platform':fp.platform,'languages':fp.languages,
            'cores':fp.hardware_concurrency,'memory':fp.device_memory,'timezone':fp.timezone,
            'toStringNative':True,'permissionsAgree':True,'canvasStable':True,
            'webglRenderer':fp.webgl_renderer,'fontKnown':True,'fontAlien':False,
            'cameras':fp.cameras,'microphones':fp.microphones}
    rows = []
    for engine, changes in [
        ('playwright',{}),
        ('playwright',{'webdriver':True,'plugins':0,'hasChrome':False,'canvasStable':False,
                       'platform':'other','cores':1,'fontAlien':True}),
        ('camoufox',{'webglRenderer':'other','fontKnown':False,'cameras':0}),
        ('cloak',{'fontKnown':None,'cameras':None}),
        ('playwright',{'fontsRendered':fp.fonts}),
    ]:
        observation = {**seen,**changes}
        rows.append({'profile':profile.to_dict(),'engine':engine,'observation':observation,
                     'expected':_grade(fp,observation,engine)})
    return {'cases':rows,'script':_TRUST_JS.strip(),'url':PROBE_URL,'html':PROBE_HTML}


def alignment_cases(parent):
    from pathlib import Path
    from unittest.mock import patch
    from numera.models import Profile
    from numera.probe.align import align_to_reality
    from numera.probe.trust import _FONT_PROBE_JS
    from numera.store import ProfileStore
    from numera.devices import FONTS
    results = []
    for index, (platform, extras) in enumerate([
        ('MacIntel', {'maxTouchPoints':0,'devicePixelRatio':2}),
        ('Linux armv8l', {'maxTouchPoints':None,'devicePixelRatio':None}),
        ('Win32', {'screenW':800,'screenH':600,'fontsRendered':[]}),
    ]):
        profile = Profile(name='Fixture',id='fixture',fingerprint=generate(seed=42,os='windows'))
        store = ProfileStore(Path(parent)/str(index));store.save(profile)
        fp = profile.fingerprint
        seen = {'webdriver':False,'webdriverOwn':False,'hasChrome':True,'plugins':5,
            'userAgent':fp.user_agent,'platform':platform,'languages':['de-DE','de'],
            'cores':4,'memory':8,'timezone':'Europe/Berlin','toStringNative':True,
            'permissionsAgree':True,'canvasStable':True,'webglRenderer':'Apple GPU',
            'webglVendor':None,'fontKnown':True,'fontAlien':False,'cameras':0,'microphones':0,
            'screenW':1920,'screenH':1080,'fontsRendered':['Arial'],**extras}
        def observe(*args,**kwargs):
            # An unrelated edit between observation and commit must survive.
            current = store.get('fixture');current.notes='edited during probe';store.save(current)
            return seen
        with patch('numera.probe.trust._observe',observe):
            expected = align_to_reality(profile,store)
        results.append({'profile':Profile(name='Fixture',id='fixture',fingerprint=generate(seed=42,os='windows')).to_dict(),
                        'observation':seen,'expected':expected,'stored':store.get('fixture').to_dict()})
    return {'cases':results,'fontScript':_FONT_PROBE_JS.strip(),
            'candidates':sorted({font for pool in FONTS.values() for font in pool})}
