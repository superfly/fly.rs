FROM node:10 as v8env

ADD fly fly

WORKDIR ./fly
RUN yarn install

WORKDIR ./fly/packages/v8env
RUN rollup -c