"""Observe original SPA/static response policy against isolated explicit build files."""
from __future__ import annotations

from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from baseline_source import activate
activate()

from numera.server import create_app, spa


def cases(parent):
    root=Path(parent)
    build=root/'web';(build/'assets').mkdir(parents=True,exist_ok=True)
    for relative,data in [('index.html','<!doctype html><title>Numera fixture</title>'),
                          ('assets/index.js','export default 1;'),('favicon.svg','<svg/>'),
                          ('hello.txt','Hello fixture')]:
        (build/relative).write_text(data,encoding='utf-8')
    result=[]
    with patch.object(spa,'dashboard_dir',return_value=build),patch.dict('os.environ',{'NUMERA_API_TOKEN':'','NUMERA_ALLOWED_ORIGINS':''}):
        with TestClient(create_app(str(root/'data'),start_scheduler=False)) as client:
            for method,path in [('GET','/'),('GET','/proxies'),('GET','/favicon.svg'),('GET','/assets/index.js'),
                                ('GET','/assets/missing.js'),('GET','/hello.txt'),('GET','/api/missing'),('POST','/proxies'),
                                ('GET','/assets'),('HEAD','/proxies'),('HEAD','/favicon.svg'),
                                ('HEAD','/assets/index.js'),('HEAD','/assets/missing.js'),('HEAD','/api/missing')]:
                response=client.request(method,path)
                result.append({'method':method,'path':path,'status':response.status_code,
                               'body':response.text,'type':response.headers.get('content-type')})
    authenticated=[]
    with patch.object(spa,'dashboard_dir',return_value=build),patch.dict('os.environ',{'NUMERA_API_TOKEN':'fixture-api-token','NUMERA_ALLOWED_ORIGINS':''}):
        with TestClient(create_app(str(root/'protected-data'),start_scheduler=False)) as client:
            for path,authorized in [('/api/missing',False),('/api/missing',True),('/',False),('/api/ready',False)]:
                headers={'Authorization':'Bearer fixture-api-token'} if authorized else {}
                response=client.get(path,headers=headers)
                authenticated.append({'method':'GET','path':path,'authorized':authorized,'status':response.status_code,
                                      'body':response.text,'type':response.headers.get('content-type')})
    return {'cases':result,'authenticated':authenticated}
