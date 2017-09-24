#!/usr/bin/false

IFS=$'\n\t'
set -euo pipefail

docker build . -f Dockerfile.$NAME -t $NAME-build
CONTAINER_ID=$(docker run -d -t $NAME-build /usr/bin/ruby ci.rb)
docker attach $CONTAINER_ID
[ $(docker wait $CONTAINER_ID) == "0" ]

docker commit $CONTAINER_ID $NAME-build-completed
FINAL_ID=$(docker run --detach -t $NAME-build-completed /bin/bash)
docker cp $FINAL_ID:/root/build/output.tar ./
tar -xf output.tar
rm -f output.tar
docker stop $FINAL_ID
docker rm $CONTAINER_ID $FINAL_ID
docker rmi $NAME-build-completed
